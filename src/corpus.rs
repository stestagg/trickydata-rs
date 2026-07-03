//! Loading a compiled corpus (`trickydata.trickydata`).
//!
//! A `.trickydata` artifact is self-contained: a magic prefix, the corpus
//! version, a checksummed gzip of the JSON index, and the raw payload blob
//! inline. The index is the source of truth for metadata; the blob holds each
//! input's raw payload, sliced out by `offset`/`length`.

use std::collections::HashMap;
use std::io::Read;

use serde::Deserialize;

use crate::decode_as::DecodeAs;
use crate::error::ParseError;

const MAGIC: &[u8] = b"trickydata:";

/// How two paired inputs relate (the `pair[].hint` value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairHint {
    /// They look different but should compare equal.
    Equal,
    /// They look the same but should compare unequal.
    NotEqual,
    /// Together they form a tricky case.
    Tricky,
}

impl PairHint {
    fn parse(s: &str) -> Option<PairHint> {
        match s {
            "equal" => Some(PairHint::Equal),
            "not-equal" => Some(PairHint::NotEqual),
            "tricky" => Some(PairHint::Tricky),
            _ => None,
        }
    }
}

/// A resolved link from one input to another in the corpus.
#[derive(Debug, Clone)]
pub struct PairLink {
    /// Name of the linked input.
    pub name: String,
    /// How the two relate.
    pub hint: PairHint,
}

/// Known-good Unicode segmentation counts, present for Unicode text inputs.
#[derive(Debug, Clone, Deserialize)]
pub struct UnicodeMeta {
    #[serde(rename = "code-units")]
    pub code_units: u64,
    #[serde(rename = "code-points")]
    pub code_points: u64,
    #[serde(rename = "scalar-values")]
    pub scalar_values: u64,
    #[serde(rename = "legacy-grapheme-clusters")]
    pub legacy_grapheme_clusters: u64,
    #[serde(rename = "extended-grapheme-clusters")]
    pub extended_grapheme_clusters: u64,
}

/// One corpus input: its metadata, raw payload, and once-decoded native value.
#[derive(Debug, Clone)]
pub struct Input {
    pub name: String,
    pub path: String,
    pub description: String,
    pub tags: Vec<String>,
    /// The stored `decode-as` (defaults to `bytes` when absent).
    pub decode_as: DecodeAs,
    /// For deliberately-invalid inputs: the type this *would* decode as if valid.
    pub invalid_as: Option<DecodeAs>,
    pub unicode_meta: Option<UnicodeMeta>,
    pub(crate) payload: Vec<u8>,
    pub(crate) links: Vec<PairLink>,
}

impl Input {
    /// True for an ordinary, valid input; false for a deliberately-invalid one.
    pub fn is_valid(&self) -> bool {
        self.invalid_as.is_none()
    }
}

/// A loaded corpus plus derived lookups.
#[derive(Debug, Clone)]
pub struct Corpus {
    pub(crate) version: String,
    pub(crate) inputs: Vec<Input>,
    pub(crate) by_name: HashMap<String, usize>,
}

impl Corpus {
    /// The corpus version recorded in the index.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// All inputs, in deterministic (name-sorted) order.
    pub fn inputs(&self) -> &[Input] {
        &self.inputs
    }

    /// Look up an input by its unique name.
    pub fn input(&self, name: &str) -> Option<&Input> {
        self.by_name.get(name).map(|&i| &self.inputs[i])
    }

    /// Build a corpus from the bytes of a self-contained `.trickydata` artifact.
    pub fn from_trickydata_bytes(data: &[u8]) -> Result<Corpus, ParseError> {
        let (doc, blob) = decode_trickydata(data)?;
        Corpus::from_index(doc, &blob)
    }

    fn from_index(doc: RawIndex, blob: &[u8]) -> Result<Corpus, ParseError> {
        let mut inputs: Vec<Input> = Vec::with_capacity(doc.inputs.len());
        for entry in &doc.inputs {
            let end = entry.offset.checked_add(entry.length);
            let payload = match end {
                Some(end) if end <= blob.len() => blob[entry.offset..end].to_vec(),
                _ => {
                    return Err(ParseError::BadSlice {
                        name: entry.name.clone(),
                        offset: entry.offset,
                        length: entry.length,
                        blob_len: blob.len(),
                    })
                }
            };

            let decode_as =
                parse_token("decode-as", entry.decode_as.as_deref())?.unwrap_or(DecodeAs::Bytes);
            let invalid_as = parse_token("invalid-as", entry.invalid_as.as_deref())?;

            inputs.push(Input {
                name: entry.name.clone(),
                path: entry.path.clone().unwrap_or_default(),
                description: entry.description.clone().unwrap_or_default(),
                tags: entry.tags.clone(),
                decode_as,
                invalid_as,
                unicode_meta: entry.unicode_meta.clone(),
                payload,
                links: Vec::new(),
            });
        }

        resolve_pairs(&mut inputs, &doc.inputs);

        let by_name = inputs
            .iter()
            .enumerate()
            .map(|(i, inp)| (inp.name.clone(), i))
            .collect();

        Ok(Corpus {
            version: doc.version.unwrap_or_else(|| "dev".to_string()),
            inputs,
            by_name,
        })
    }
}

/// Decode the `.trickydata` byte layout, verifying the repeated md5 checksum.
fn decode_trickydata(data: &[u8]) -> Result<(RawIndex, Vec<u8>), ParseError> {
    let mut p = data
        .strip_prefix(MAGIC)
        .ok_or_else(|| trickydata_error("missing 'trickydata:' magic prefix"))?;

    let zero = p
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| trickydata_error("missing version terminator"))?;
    let version = std::str::from_utf8(&p[..zero])
        .map_err(|_| trickydata_error("version string is not valid UTF-8"))?
        .to_string();
    p = &p[zero + 1..];

    let leading = take(&mut p, 16, "leading checksum")?;
    let len = u32::from_le_bytes(
        take(&mut p, 4, "index length")?
            .try_into()
            .expect("take returned four bytes"),
    ) as usize;
    let gz = take(&mut p, len, "compressed index")?;
    if md5::compute(gz).0 != leading {
        return Err(trickydata_error(
            "index checksum mismatch (corrupt .trickydata)",
        ));
    }
    let trailing = take(&mut p, 16, "trailing checksum")?;
    if trailing != leading {
        return Err(trickydata_error(
            "trailing index checksum mismatch (corrupt .trickydata)",
        ));
    }
    if take(&mut p, 1, "blob separator")?[0] != 0 {
        return Err(trickydata_error("expected zero separator before blob"));
    }

    let mut index_bytes = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut index_bytes)
        .map_err(|e| trickydata_error(format!("decompressing index: {e}")))?;
    let doc: RawIndex = serde_json::from_slice(&index_bytes)?;
    let index_version = doc.version.as_deref().unwrap_or("dev");
    if index_version != version {
        return Err(trickydata_error(format!(
            "header version {version:?} does not match index version {index_version:?}"
        )));
    }

    Ok((doc, p.to_vec()))
}

fn take<'a>(p: &mut &'a [u8], n: usize, what: &str) -> Result<&'a [u8], ParseError> {
    if p.len() < n {
        return Err(trickydata_error(format!(
            "truncated .trickydata: expected {n} more byte(s) for {what}"
        )));
    }
    let (head, tail) = p.split_at(n);
    *p = tail;
    Ok(head)
}

fn trickydata_error(message: impl Into<String>) -> ParseError {
    ParseError::Trickydata(message.into())
}

/// Parse an optional `decode-as`/`invalid-as` token, erroring on an unknown one
/// rather than silently treating it as bytes.
fn parse_token(field: &'static str, token: Option<&str>) -> Result<Option<DecodeAs>, ParseError> {
    match token {
        None => Ok(None),
        Some(s) => s
            .parse::<DecodeAs>()
            .map(Some)
            .map_err(|()| ParseError::UnknownToken {
                field,
                token: s.to_string(),
            }),
    }
}

/// Resolve each `pair[].with` (a path relative to the linking input's own
/// `path`) to the target input's name, and make the link symmetric. The index
/// already records symmetric links, but we resolve defensively so a one-sided
/// declaration still links both ways. Matches on the lexically-normalized path.
fn resolve_pairs(inputs: &mut [Input], raw: &[RawEntry]) {
    let name_by_path: HashMap<String, String> = inputs
        .iter()
        .filter(|i| !i.path.is_empty())
        .map(|i| (norm(&i.path), i.name.clone()))
        .collect();
    let index_by_name: HashMap<String, usize> = inputs
        .iter()
        .enumerate()
        .map(|(i, inp)| (inp.name.clone(), i))
        .collect();

    // Collect (source_name, target_name, hint) first to avoid aliasing `inputs`.
    let mut resolved: Vec<(String, String, PairHint)> = Vec::new();
    for (entry, inp) in raw.iter().zip(inputs.iter()) {
        let base_dir = posix_dirname(&inp.path);
        for link in entry.pair.iter().flatten() {
            let Some(hint) = PairHint::parse(&link.hint) else {
                continue;
            };
            let target_path = norm(&posix_join(&base_dir, &link.with));
            if let Some(target_name) = name_by_path.get(&target_path) {
                resolved.push((inp.name.clone(), target_name.clone(), hint));
            }
        }
    }

    for (src, dst, hint) in resolved {
        if let Some(&i) = index_by_name.get(&src) {
            if !inputs[i].links.iter().any(|l| l.name == dst) {
                inputs[i].links.push(PairLink { name: dst, hint });
            }
        }
    }

    // Stable order for deterministic output.
    for inp in inputs.iter_mut() {
        inp.links.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

// --- forward-slash path helpers ----------------------------------------------

/// Lexically normalize a forward-slash path, resolving `.`/`..` without touching
/// the filesystem.
fn norm(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut out: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                if matches!(out.last(), Some(&c) if c != "..") {
                    out.pop();
                } else {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

fn posix_dirname(path: &str) -> String {
    let path = path.replace('\\', "/");
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

fn posix_join(base: &str, rel: &str) -> String {
    if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    }
}

// --- raw index deserialization ----------------------------------------------

#[derive(Deserialize)]
struct RawIndex {
    version: Option<String>,
    #[serde(default)]
    inputs: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    name: String,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(rename = "decode-as")]
    decode_as: Option<String>,
    #[serde(rename = "invalid-as")]
    invalid_as: Option<String>,
    #[serde(default)]
    pair: Option<Vec<RawPair>>,
    #[serde(rename = "unicode-meta")]
    unicode_meta: Option<UnicodeMeta>,
    path: Option<String>,
    offset: usize,
    length: usize,
}

#[derive(Deserialize)]
struct RawPair {
    with: String,
    hint: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_resolves_dot_dot() {
        assert_eq!(norm("../inputs/utf8/./a.input"), "../inputs/utf8/a.input");
        assert_eq!(norm("a/b/../c"), "a/c");
        assert_eq!(norm("a/b/c/../../d"), "a/d");
    }
}
