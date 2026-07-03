//! Compile-time literals for the trickydata corpus.

use std::io::Read;

use proc_macro::{Delimiter, Literal, TokenStream, TokenTree};
use serde::Deserialize;

const ARTIFACT: &[u8] = include_bytes!("../../data/trickydata.trickydata");
const MAGIC: &[u8] = b"trickydata:";

#[proc_macro]
pub fn static_values(input: TokenStream) -> TokenStream {
    expand(input, Mode::Values)
}

#[proc_macro]
pub fn static_examples(input: TokenStream) -> TokenStream {
    expand(input, Mode::Examples)
}

fn expand(input: TokenStream, mode: Mode) -> TokenStream {
    match expand_result(input, mode) {
        Ok(out) => out.parse().expect("generated valid Rust"),
        Err(err) => compile_error(&err),
    }
}

fn compile_error(message: &str) -> TokenStream {
    format!("compile_error!({});", string_lit(message))
        .parse()
        .expect("compile_error is valid Rust")
}

fn expand_result(input: TokenStream, mode: Mode) -> Result<String, String> {
    let args = Args::parse(input)?;
    let target = Target::parse(&args.target)?;
    let corpus = Corpus::load()?;

    let mut values = Vec::new();
    let mut examples = Vec::new();
    for entry in &corpus.entries {
        let Some(selection) = select(entry, target, &args) else {
            continue;
        };
        let value = value_expr(entry, target, selection)?;
        match mode {
            Mode::Values => {
                if let Some(value) = value {
                    values.push(value);
                }
            }
            Mode::Examples => {
                examples.push(example_expr(entry, target, value)?);
            }
        }
    }

    Ok(match mode {
        Mode::Values => format!(
            "&[{}] as &'static [{}]",
            values.join(", "),
            target.value_type()
        ),
        Mode::Examples => format!(
            "&[{}] as &'static [::trickydata::StaticExample<{}>]",
            examples.join(", "),
            target.value_type()
        ),
    })
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Values,
    Examples,
}

#[derive(Debug)]
struct Args {
    target: String,
    tags: Vec<String>,
    exclude_tags: Vec<String>,
    names: Vec<String>,
    promote: bool,
    include_invalid: bool,
}

impl Args {
    fn parse(input: TokenStream) -> Result<Self, String> {
        let tokens: Vec<TokenTree> = input.into_iter().collect();
        let split = tokens
            .iter()
            .position(|token| matches!(token, TokenTree::Punct(p) if p.as_char() == ','));
        let (target_tokens, option_tokens) = match split {
            Some(i) => (&tokens[..i], &tokens[i + 1..]),
            None => (&tokens[..], &[][..]),
        };
        if target_tokens.is_empty() {
            return Err("expected target type, e.g. String or f64".to_string());
        }

        let mut args = Args {
            target: compact_tokens(target_tokens),
            tags: Vec::new(),
            exclude_tags: Vec::new(),
            names: Vec::new(),
            promote: true,
            include_invalid: true,
        };
        args.parse_options(option_tokens)?;
        Ok(args)
    }

    fn parse_options(&mut self, tokens: &[TokenTree]) -> Result<(), String> {
        let mut i = 0;
        while i < tokens.len() {
            if matches!(&tokens[i], TokenTree::Punct(p) if p.as_char() == ',') {
                i += 1;
                continue;
            }
            let key = match &tokens[i] {
                TokenTree::Ident(ident) => ident.to_string(),
                other => return Err(format!("expected option name, got {other}")),
            };
            i += 1;
            match tokens.get(i) {
                Some(TokenTree::Punct(p)) if p.as_char() == '=' => i += 1,
                _ => return Err(format!("expected `=` after `{key}`")),
            }
            let value = tokens
                .get(i)
                .ok_or_else(|| format!("expected value for `{key}`"))?;
            match key.as_str() {
                "tags" => self.tags = parse_string_array(value, "tags")?,
                "exclude_tags" => self.exclude_tags = parse_string_array(value, "exclude_tags")?,
                "names" => self.names = parse_string_array(value, "names")?,
                "promote" => self.promote = parse_bool(value, "promote")?,
                "include_invalid" => self.include_invalid = parse_bool(value, "include_invalid")?,
                _ => return Err(format!("unknown option `{key}`")),
            }
            i += 1;
        }
        Ok(())
    }
}

fn compact_tokens(tokens: &[TokenTree]) -> String {
    tokens
        .iter()
        .map(ToString::to_string)
        .collect::<String>()
        .replace(' ', "")
}

fn parse_string_array(token: &TokenTree, name: &str) -> Result<Vec<String>, String> {
    let TokenTree::Group(group) = token else {
        return Err(format!("`{name}` must be a string array"));
    };
    if group.delimiter() != Delimiter::Bracket {
        return Err(format!("`{name}` must be a string array"));
    }
    let mut out = Vec::new();
    for token in group.stream() {
        match token {
            TokenTree::Literal(lit) => out.push(parse_string_lit(&lit)?),
            TokenTree::Punct(p) if p.as_char() == ',' => {}
            other => return Err(format!("expected string literal in `{name}`, got {other}")),
        }
    }
    Ok(out)
}

fn parse_string_lit(lit: &Literal) -> Result<String, String> {
    let text = lit.to_string();
    serde_json::from_str(&text).map_err(|e| format!("invalid string literal {text}: {e}"))
}

fn parse_bool(token: &TokenTree, name: &str) -> Result<bool, String> {
    match token {
        TokenTree::Ident(ident) if ident.to_string() == "true" => Ok(true),
        TokenTree::Ident(ident) if ident.to_string() == "false" => Ok(false),
        other => Err(format!("`{name}` must be true or false, got {other}")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    Bool,
    String,
    Bytes,
}

impl Target {
    fn parse(s: &str) -> Result<Target, String> {
        Ok(match s {
            "u8" => Target::U8,
            "u16" => Target::U16,
            "u32" => Target::U32,
            "u64" => Target::U64,
            "u128" => Target::U128,
            "i8" => Target::I8,
            "i16" => Target::I16,
            "i32" => Target::I32,
            "i64" => Target::I64,
            "i128" => Target::I128,
            "f32" => Target::F32,
            "f64" => Target::F64,
            "bool" => Target::Bool,
            "String" => Target::String,
            "Vec<u8>" => Target::Bytes,
            _ => {
                return Err(format!(
                    "unsupported static target `{s}`; expected String, Vec<u8>, bool, an integer, f32, or f64"
                ))
            }
        })
    }

    fn value_type(self) -> &'static str {
        match self {
            Target::U8 => "u8",
            Target::U16 => "u16",
            Target::U32 => "u32",
            Target::U64 => "u64",
            Target::U128 => "u128",
            Target::I8 => "i8",
            Target::I16 => "i16",
            Target::I32 => "i32",
            Target::I64 => "i64",
            Target::I128 => "i128",
            Target::F32 => "f32",
            Target::F64 => "f64",
            Target::Bool => "bool",
            Target::String => "&'static str",
            Target::Bytes => "&'static [u8]",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum DecodeAs {
    U8,
    #[serde(rename = "u16-le")]
    U16Le,
    #[serde(rename = "u32-le")]
    U32Le,
    #[serde(rename = "u64-le")]
    U64Le,
    #[serde(rename = "u128-le")]
    U128Le,
    I8,
    #[serde(rename = "i16-le")]
    I16Le,
    #[serde(rename = "i32-le")]
    I32Le,
    #[serde(rename = "i64-le")]
    I64Le,
    #[serde(rename = "i128-le")]
    I128Le,
    #[serde(rename = "f16-le")]
    F16Le,
    #[serde(rename = "f32-le")]
    F32Le,
    #[serde(rename = "f64-le")]
    F64Le,
    Bool,
    Bytes,
    Ascii,
    Utf8,
    #[serde(rename = "utf16-le")]
    Utf16Le,
    #[serde(rename = "utf32-le")]
    Utf32Le,
    Latin1,
}

impl DecodeAs {
    fn rust(self) -> &'static str {
        match self {
            DecodeAs::U8 => "::trickydata::DecodeAs::U8",
            DecodeAs::U16Le => "::trickydata::DecodeAs::U16Le",
            DecodeAs::U32Le => "::trickydata::DecodeAs::U32Le",
            DecodeAs::U64Le => "::trickydata::DecodeAs::U64Le",
            DecodeAs::U128Le => "::trickydata::DecodeAs::U128Le",
            DecodeAs::I8 => "::trickydata::DecodeAs::I8",
            DecodeAs::I16Le => "::trickydata::DecodeAs::I16Le",
            DecodeAs::I32Le => "::trickydata::DecodeAs::I32Le",
            DecodeAs::I64Le => "::trickydata::DecodeAs::I64Le",
            DecodeAs::I128Le => "::trickydata::DecodeAs::I128Le",
            DecodeAs::F16Le => "::trickydata::DecodeAs::F16Le",
            DecodeAs::F32Le => "::trickydata::DecodeAs::F32Le",
            DecodeAs::F64Le => "::trickydata::DecodeAs::F64Le",
            DecodeAs::Bool => "::trickydata::DecodeAs::Bool",
            DecodeAs::Bytes => "::trickydata::DecodeAs::Bytes",
            DecodeAs::Ascii => "::trickydata::DecodeAs::Ascii",
            DecodeAs::Utf8 => "::trickydata::DecodeAs::Utf8",
            DecodeAs::Utf16Le => "::trickydata::DecodeAs::Utf16Le",
            DecodeAs::Utf32Le => "::trickydata::DecodeAs::Utf32Le",
            DecodeAs::Latin1 => "::trickydata::DecodeAs::Latin1",
        }
    }

    fn is_text(self) -> bool {
        matches!(
            self,
            DecodeAs::Ascii
                | DecodeAs::Utf8
                | DecodeAs::Utf16Le
                | DecodeAs::Utf32Le
                | DecodeAs::Latin1
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum Selection {
    Native,
    Invalid,
}

fn select(entry: &Entry, target: Target, args: &Args) -> Option<Selection> {
    if !args.names.is_empty() && !args.names.iter().any(|name| name == &entry.name) {
        return None;
    }
    if !args
        .tags
        .iter()
        .all(|tag| entry.tags.iter().any(|entry_tag| entry_tag == tag))
    {
        return None;
    }
    if args
        .exclude_tags
        .iter()
        .any(|tag| entry.tags.iter().any(|entry_tag| entry_tag == tag))
    {
        return None;
    }

    if accepts(target, entry.decode_as, args.promote) {
        return Some(Selection::Native);
    }
    if args.include_invalid && entry.invalid_as.is_some_and(|invalid| accepts(target, invalid, false)) {
        return Some(Selection::Invalid);
    }
    None
}

fn accepts(target: Target, src: DecodeAs, promote: bool) -> bool {
    use DecodeAs::*;
    match target {
        Target::U8 => src == U8,
        Target::U16 => matches_promote(src, promote, U16Le, &[U8, U16Le]),
        Target::U32 => matches_promote(src, promote, U32Le, &[U8, U16Le, U32Le]),
        Target::U64 => matches_promote(src, promote, U64Le, &[U8, U16Le, U32Le, U64Le]),
        Target::U128 => matches_promote(src, promote, U128Le, &[U8, U16Le, U32Le, U64Le, U128Le]),
        Target::I8 => src == I8,
        Target::I16 => matches_promote(src, promote, I16Le, &[I8, I16Le, U8]),
        Target::I32 => matches_promote(src, promote, I32Le, &[I8, I16Le, I32Le, U8, U16Le]),
        Target::I64 => matches_promote(
            src,
            promote,
            I64Le,
            &[I8, I16Le, I32Le, I64Le, U8, U16Le, U32Le],
        ),
        Target::I128 => matches_promote(
            src,
            promote,
            I128Le,
            &[I8, I16Le, I32Le, I64Le, I128Le, U8, U16Le, U32Le, U64Le],
        ),
        Target::F32 => matches_promote(src, promote, F32Le, &[F16Le, F32Le]),
        Target::F64 => matches_promote(src, promote, F64Le, &[F16Le, F32Le, F64Le]),
        Target::Bool => src == Bool,
        Target::String => src.is_text(),
        Target::Bytes => promote || src == Bytes,
    }
}

fn matches_promote(src: DecodeAs, promote: bool, native: DecodeAs, promoted: &[DecodeAs]) -> bool {
    if promote {
        promoted.contains(&src)
    } else {
        src == native
    }
}

fn value_expr(
    entry: &Entry,
    target: Target,
    selection: Selection,
) -> Result<Option<String>, String> {
    if matches!(selection, Selection::Invalid) {
        return Ok(None);
    }
    Ok(match target {
        Target::String => Some(string_value_expr(entry.decode_as, &entry.payload)?),
        Target::Bytes => Some(bytes_value_expr(&entry.payload)),
        Target::Bool => Some(entry.payload.iter().any(|&b| b != 0).to_string()),
        Target::F32 | Target::F64 => Some(float_value_expr(target, entry.decode_as, &entry.payload)?),
        Target::U8 | Target::U16 | Target::U32 | Target::U64 | Target::U128 => {
            let value = decode_uint(entry.decode_as, &entry.payload)?;
            Some(uint_lit(value, target))
        }
        Target::I8 | Target::I16 | Target::I32 | Target::I64 | Target::I128 => {
            let value = match entry.decode_as {
                DecodeAs::U8
                | DecodeAs::U16Le
                | DecodeAs::U32Le
                | DecodeAs::U64Le
                | DecodeAs::U128Le => decode_uint(entry.decode_as, &entry.payload)? as i128,
                _ => decode_int(entry.decode_as, &entry.payload)?,
            };
            Some(int_lit(value, target))
        }
    })
}

fn example_expr(entry: &Entry, target: Target, value: Option<String>) -> Result<String, String> {
    let tags = entry
        .tags
        .iter()
        .map(|tag| string_lit(tag))
        .collect::<Vec<_>>()
        .join(", ");
    let invalid_as = match entry.invalid_as {
        Some(invalid) => format!("Some({})", invalid.rust()),
        None => "None".to_string(),
    };
    let value = match value {
        Some(value) => format!("Some({value})"),
        None => "None".to_string(),
    };
    Ok(format!(
        "::trickydata::StaticExample::<{}> {{ name: {}, path: {}, description: {}, tags: &[{}] as &'static [&'static str], decode_as: {}, invalid_as: {}, value: {}, raw: {} }}",
        target.value_type(),
        string_lit(&entry.name),
        string_lit(&entry.path),
        string_lit(&entry.description),
        tags,
        entry.decode_as.rust(),
        invalid_as,
        value,
        bytes_value_expr(&entry.payload),
    ))
}

fn string_value_expr(decode_as: DecodeAs, payload: &[u8]) -> Result<String, String> {
    let value = decode_text(decode_as, payload)?;
    Ok(string_lit(&value))
}

fn bytes_value_expr(payload: &[u8]) -> String {
    format!("&[{}] as &'static [u8]", bytes_lit_body(payload))
}

fn float_value_expr(target: Target, decode_as: DecodeAs, payload: &[u8]) -> Result<String, String> {
    match (target, decode_as) {
        (Target::F32, DecodeAs::F16Le) => Ok(format!("f32::from_bits(0x{:08x})", f16_to_f32_bits(decode_u16(decode_as, payload)?))),
        (Target::F32, DecodeAs::F32Le) => Ok(format!("f32::from_bits(0x{:08x})", decode_u32(decode_as, payload)?)),
        (Target::F64, DecodeAs::F16Le) => Ok(format!("f32::from_bits(0x{:08x}) as f64", f16_to_f32_bits(decode_u16(decode_as, payload)?))),
        (Target::F64, DecodeAs::F32Le) => Ok(format!("f32::from_bits(0x{:08x}) as f64", decode_u32(decode_as, payload)?)),
        (Target::F64, DecodeAs::F64Le) => Ok(format!("f64::from_bits(0x{:016x})", decode_u64(decode_as, payload)?)),
        _ => Err(format!("cannot emit {decode_as:?} as {target:?}")),
    }
}

fn uint_lit(value: u128, target: Target) -> String {
    format!("{value}{}", target.value_type())
}

fn int_lit(value: i128, target: Target) -> String {
    let ty = target.value_type();
    let min = match target {
        Target::I8 => i8::MIN as i128,
        Target::I16 => i16::MIN as i128,
        Target::I32 => i32::MIN as i128,
        Target::I64 => i64::MIN as i128,
        Target::I128 => i128::MIN,
        _ => unreachable!("int_lit called for non-signed target"),
    };
    if value == min {
        format!("{ty}::MIN")
    } else {
        format!("{value}{ty}")
    }
}

fn string_lit(s: &str) -> String {
    format!("{s:?}")
}

fn bytes_lit_body(payload: &[u8]) -> String {
    payload
        .iter()
        .map(|b| format!("0x{b:02x}u8"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn decode_uint(decode_as: DecodeAs, payload: &[u8]) -> Result<u128, String> {
    require_width(decode_as, payload)?;
    let mut bytes = [0u8; 16];
    bytes[..payload.len()].copy_from_slice(payload);
    Ok(u128::from_le_bytes(bytes))
}

fn decode_int(decode_as: DecodeAs, payload: &[u8]) -> Result<i128, String> {
    let width = require_width(decode_as, payload)?;
    let mut bytes = [0u8; 16];
    bytes[..width].copy_from_slice(payload);
    if width < 16 && payload[width - 1] & 0x80 != 0 {
        for b in &mut bytes[width..] {
            *b = 0xff;
        }
    }
    Ok(i128::from_le_bytes(bytes))
}

fn decode_u16(decode_as: DecodeAs, payload: &[u8]) -> Result<u16, String> {
    require_width(decode_as, payload)?;
    Ok(u16::from_le_bytes([payload[0], payload[1]]))
}

fn decode_u32(decode_as: DecodeAs, payload: &[u8]) -> Result<u32, String> {
    require_width(decode_as, payload)?;
    Ok(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]))
}

fn decode_u64(decode_as: DecodeAs, payload: &[u8]) -> Result<u64, String> {
    require_width(decode_as, payload)?;
    Ok(u64::from_le_bytes([
        payload[0], payload[1], payload[2], payload[3], payload[4], payload[5], payload[6],
        payload[7],
    ]))
}

fn require_width(decode_as: DecodeAs, payload: &[u8]) -> Result<usize, String> {
    let expected = match decode_as {
        DecodeAs::U8 | DecodeAs::I8 => 1,
        DecodeAs::U16Le | DecodeAs::I16Le | DecodeAs::F16Le => 2,
        DecodeAs::U32Le | DecodeAs::I32Le | DecodeAs::F32Le => 4,
        DecodeAs::U64Le | DecodeAs::I64Le | DecodeAs::F64Le => 8,
        DecodeAs::U128Le | DecodeAs::I128Le => 16,
        _ => return Err(format!("{decode_as:?} is not numeric")),
    };
    if payload.len() != expected {
        return Err(format!(
            "{decode_as:?} expected {expected} byte(s), got {}",
            payload.len()
        ));
    }
    Ok(expected)
}

fn f16_to_f32_bits(bits: u16) -> u32 {
    let sign = (bits >> 15) & 0x1;
    let exp = (bits >> 10) & 0x1f;
    let mant = bits & 0x03ff;

    match exp {
        0 => {
            if mant == 0 {
                (sign as u32) << 31
            } else {
                let p = (15 - mant.leading_zeros()) as i32;
                let frac = (mant ^ (1 << p)) as u32;
                let f32_exp = (p - 24 + 127) as u32;
                ((sign as u32) << 31) | (f32_exp << 23) | (frac << (23 - p as u32))
            }
        }
        0x1f => ((sign as u32) << 31) | (0xff << 23) | ((mant as u32) << 13),
        _ => {
            let f32_exp = (exp as i32 - 15 + 127) as u32;
            ((sign as u32) << 31) | (f32_exp << 23) | ((mant as u32) << 13)
        }
    }
}

fn decode_text(decode_as: DecodeAs, payload: &[u8]) -> Result<String, String> {
    match decode_as {
        DecodeAs::Ascii => {
            if let Some(pos) = payload.iter().position(|&b| b >= 0x80) {
                return Err(format!("non-ASCII byte at index {pos}"));
            }
            Ok(String::from_utf8(payload.to_vec()).expect("ascii is valid utf8"))
        }
        DecodeAs::Utf8 => std::str::from_utf8(payload)
            .map(str::to_owned)
            .map_err(|e| e.to_string()),
        DecodeAs::Latin1 => Ok(payload.iter().map(|&b| b as char).collect()),
        DecodeAs::Utf16Le => {
            if payload.len() % 2 != 0 {
                return Err(format!("byte length {} is not a multiple of 2", payload.len()));
            }
            let units = payload
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]));
            char::decode_utf16(units)
                .collect::<Result<String, _>>()
                .map_err(|e| format!("unpaired surrogate 0x{:04x}", e.unpaired_surrogate()))
        }
        DecodeAs::Utf32Le => {
            if payload.len() % 4 != 0 {
                return Err(format!("byte length {} is not a multiple of 4", payload.len()));
            }
            payload
                .chunks_exact(4)
                .map(|c| {
                    let v = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    char::from_u32(v).ok_or_else(|| format!("0x{v:08x} is not a scalar value"))
                })
                .collect()
        }
        _ => Err(format!("{decode_as:?} is not text")),
    }
}

#[derive(Debug)]
struct Corpus {
    entries: Vec<Entry>,
}

impl Corpus {
    fn load() -> Result<Corpus, String> {
        let (index, blob) = decode_trickydata(ARTIFACT)?;
        let mut entries = Vec::with_capacity(index.inputs.len());
        for raw in index.inputs {
            let end = raw
                .offset
                .checked_add(raw.length)
                .ok_or_else(|| format!("input {:?} has overflowing slice", raw.name))?;
            let payload = blob
                .get(raw.offset..end)
                .ok_or_else(|| format!("input {:?} slice is out of bounds", raw.name))?
                .to_vec();
            entries.push(Entry {
                name: raw.name,
                path: raw.path.unwrap_or_default(),
                description: raw.description.unwrap_or_default(),
                tags: raw.tags,
                decode_as: raw.decode_as.unwrap_or(DecodeAs::Bytes),
                invalid_as: raw.invalid_as,
                payload,
            });
        }
        Ok(Corpus { entries })
    }
}

#[derive(Debug)]
struct Entry {
    name: String,
    path: String,
    description: String,
    tags: Vec<String>,
    decode_as: DecodeAs,
    invalid_as: Option<DecodeAs>,
    payload: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct RawIndex {
    version: Option<String>,
    #[serde(default)]
    inputs: Vec<RawEntry>,
}

#[derive(Debug, Deserialize)]
struct RawEntry {
    name: String,
    description: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(rename = "decode-as")]
    decode_as: Option<DecodeAs>,
    #[serde(rename = "invalid-as")]
    invalid_as: Option<DecodeAs>,
    path: Option<String>,
    offset: usize,
    length: usize,
}

fn decode_trickydata(data: &[u8]) -> Result<(RawIndex, Vec<u8>), String> {
    let mut p = data
        .strip_prefix(MAGIC)
        .ok_or_else(|| "missing 'trickydata:' magic prefix".to_string())?;

    let zero = p
        .iter()
        .position(|&b| b == 0)
        .ok_or_else(|| "missing version terminator".to_string())?;
    let version = std::str::from_utf8(&p[..zero])
        .map_err(|_| "version string is not valid UTF-8".to_string())?
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
        return Err("index checksum mismatch".to_string());
    }
    let trailing = take(&mut p, 16, "trailing checksum")?;
    if trailing != leading {
        return Err("trailing index checksum mismatch".to_string());
    }
    if take(&mut p, 1, "blob separator")?[0] != 0 {
        return Err("expected zero separator before blob".to_string());
    }

    let mut index_bytes = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut index_bytes)
        .map_err(|e| format!("decompressing index: {e}"))?;
    let index: RawIndex =
        serde_json::from_slice(&index_bytes).map_err(|e| format!("parsing index: {e}"))?;
    let index_version = index.version.as_deref().unwrap_or("dev");
    if index_version != version {
        return Err(format!(
            "header version {version:?} does not match index version {index_version:?}"
        ));
    }
    Ok((index, p.to_vec()))
}

fn take<'a>(p: &mut &'a [u8], n: usize, what: &str) -> Result<&'a [u8], String> {
    if p.len() < n {
        return Err(format!("truncated .trickydata: expected {n} bytes for {what}"));
    }
    let (head, tail) = p.split_at(n);
    *p = tail;
    Ok(head)
}
