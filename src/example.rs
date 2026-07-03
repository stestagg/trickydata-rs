//! The [`Example`] result object: one tricky input surfaced through a target type.
//!
//! It owns a copy of the input's data so derived examples (e.g. from string
//! sizing) are independent, and is generic over the target type `T` so
//! [`Example::value`] returns the concrete value the caller already asked for.

use std::fmt;
use std::fs;
use std::io::{self, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::corpus::{Input, PairLink, UnicodeMeta};
use crate::decode_as::DecodeAs;
use crate::error::{DecodeError, NotTextError};
use crate::native::{decode_native, Native};
use crate::target::Target;

/// One corpus input, surfaced through the target type `T`.
///
/// The same underlying input can appear under more than one target (e.g. a utf8
/// string appears as `Example<String>` and, as bytes, `Example<Vec<u8>>`).
#[derive(Clone)]
pub struct Example<T> {
    name: String,
    decode_as: DecodeAs,
    invalid_as: Option<DecodeAs>,
    description: String,
    tags: Vec<String>,
    unicode_meta: Option<UnicodeMeta>,
    raw: Vec<u8>,
    links: Vec<PairLink>,
    _t: PhantomData<fn() -> T>,
}

impl<T: Target> Example<T> {
    pub(crate) fn from_input(input: &Input) -> Example<T> {
        Example {
            name: input.name.clone(),
            decode_as: input.decode_as,
            invalid_as: input.invalid_as,
            description: input.description.clone(),
            tags: input.tags.clone(),
            unicode_meta: input.unicode_meta.clone(),
            raw: input.payload.clone(),
            links: input.links.clone(),
            _t: PhantomData,
        }
    }

    /// Construct a derived example (used by string sizing). Pairing is dropped
    /// because a transformed payload no longer satisfies the original links.
    pub(crate) fn derived(
        name: String,
        decode_as: DecodeAs,
        raw: Vec<u8>,
        description: String,
        tags: Vec<String>,
        unicode_meta: Option<UnicodeMeta>,
    ) -> Example<T> {
        Example {
            name,
            decode_as,
            invalid_as: None,
            description,
            tags,
            unicode_meta,
            raw,
            links: Vec::new(),
            _t: PhantomData,
        }
    }

    // --- values ----------------------------------------------------------

    /// The target value for this view of the input.
    ///
    /// For a deliberately-invalid input this *attempts the real decode against
    /// its would-be type* and therefore returns `Err` (e.g. invalid UTF-8, a
    /// truncated float) — it never silently hands back bytes. Use
    /// [`Example::as_bytes`] to reach the raw payload regardless.
    pub fn value(&self) -> Result<T, DecodeError> {
        // Which type to decode against depends on the lens `T`. When `T` accepts
        // the input's own `decode-as`, it is a genuine member and decodes as
        // itself — so a deliberately-invalid input viewed through the `Vec<u8>`
        // sink (a native `bytes` member) yields its raw bytes, never an error.
        // Only when the input is surfaced *as its would-be type* (the invalid
        // branch of selection) do we decode against `invalid-as` and surface the
        // deliberate failure.
        let effective = if T::accepts(self.decode_as, true) {
            self.decode_as
        } else {
            self.invalid_as.unwrap_or(self.decode_as)
        };
        let native = decode_native(effective, &self.raw)?;
        Ok(T::extract(&native, &self.raw)
            .expect("a queried example is always extractable as its target type"))
    }

    /// False for a deliberately-invalid input (one carrying `invalid-as`).
    pub fn is_valid(&self) -> bool {
        self.invalid_as.is_none()
    }

    /// True if the input's own `decode-as` is a text encoding.
    pub fn is_text(&self) -> bool {
        self.decode_as.is_text()
    }

    /// Decode as text using the input's own encoding. Text inputs only;
    /// deliberately-invalid inputs are stored as `bytes`, so this returns
    /// `Err` for them.
    pub fn as_text(&self) -> Result<String, NotTextError> {
        if !self.is_text() {
            return Err(NotTextError {
                name: self.name.clone(),
                decode_as: self.decode_as,
            });
        }
        match decode_native(self.decode_as, &self.raw) {
            Ok(Native::Text(s)) => Ok(s),
            _ => Err(NotTextError {
                name: self.name.clone(),
                decode_as: self.decode_as,
            }),
        }
    }

    /// The input's raw payload — already its correctly-encoded bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.raw
    }

    // --- metadata --------------------------------------------------------

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn decode_as(&self) -> DecodeAs {
        self.decode_as
    }

    /// The would-be type, for a deliberately-invalid input.
    pub fn invalid_as(&self) -> Option<DecodeAs> {
        self.invalid_as
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    /// Known-good Unicode segmentation counts, when the corpus provides them.
    pub fn unicode_meta(&self) -> Option<&UnicodeMeta> {
        self.unicode_meta.as_ref()
    }

    /// Resolved links to other inputs explicitly paired with this one.
    pub fn pairs(&self) -> &[PairLink] {
        &self.links
    }

    // --- file materialisation -------------------------------------------

    /// Write the input's bytes to a temporary file and return an RAII handle
    /// that deletes the file when dropped.
    pub fn as_file(&self) -> io::Result<TempFile> {
        TempFile::write(&self.raw)
    }
}

impl<T: Target> fmt::Display for Example<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Ok(s) = self.as_text() {
            f.write_str(&s)
        } else {
            // Non-text (incl. invalid) inputs: show the raw bytes rather than failing.
            write!(f, "{:?}", self.raw)
        }
    }
}

impl<T: Target> fmt::Debug for Example<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Example")
            .field("name", &self.name)
            .field("decode_as", &self.decode_as)
            .field("invalid_as", &self.invalid_as)
            .finish()
    }
}

/// A temporary file that unlinks itself on drop.
pub struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn write(data: &[u8]) -> io::Result<TempFile> {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::time::{SystemTime, UNIX_EPOCH};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("trickydata-{}-{nanos}-{n}.tmp", std::process::id());
        let path = std::env::temp_dir().join(name);

        let mut file = fs::File::create(&path)?;
        file.write_all(data)?;
        file.sync_all()?;
        Ok(TempFile { path })
    }

    /// The path to the temporary file. Valid until this handle is dropped.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

impl AsRef<Path> for TempFile {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}
