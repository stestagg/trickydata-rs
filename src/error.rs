//! Error types for the trickydata client.
//!
//! These are hand-written (no `thiserror`) to keep the dependency surface to
//! just `serde`/`serde_json`. Each implements `std::error::Error` + `Display`.

use std::fmt;

use crate::decode_as::DecodeAs;

/// A payload could not be decoded into its declared (or would-be) type.
///
/// Raised by [`crate::Example::value`] for deliberately-invalid inputs: the
/// value never silently falls back to bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The payload was not the exact width an integer/float decode requires
    /// (numerics are little-endian with no padding).
    WrongLength {
        decode_as: DecodeAs,
        expected: usize,
        got: usize,
    },
    /// The bytes were not valid in the declared text encoding.
    InvalidText { decode_as: DecodeAs, detail: String },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::WrongLength {
                decode_as,
                expected,
                got,
            } => write!(
                f,
                "cannot decode as {decode_as}: expected {expected} byte(s), got {got}"
            ),
            DecodeError::InvalidText { decode_as, detail } => {
                write!(f, "invalid {decode_as} text: {detail}")
            }
        }
    }
}

impl std::error::Error for DecodeError {}

/// `Example::as_text()` was called on a non-text input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotTextError {
    pub name: String,
    pub decode_as: DecodeAs,
}

impl fmt::Display for NotTextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:?} (decode-as {}) is not text",
            self.name, self.decode_as
        )
    }
}

impl std::error::Error for NotTextError {}

/// A compiled corpus artifact could not be parsed or loaded.
#[derive(Debug)]
pub enum ParseError {
    /// The index JSON was malformed or did not match the expected shape.
    Json(serde_json::Error),
    /// A `.trickydata` container was malformed, corrupt, or could not be inflated.
    Trickydata(String),
    /// An input referenced a `decode-as`/`invalid-as` token the client does not know.
    UnknownToken { field: &'static str, token: String },
    /// An input's `offset`/`length` fell outside the payload blob.
    BadSlice {
        name: String,
        offset: usize,
        length: usize,
        blob_len: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::Json(e) => write!(f, "parsing trickydata index: {e}"),
            ParseError::Trickydata(e) => write!(f, "parsing .trickydata artifact: {e}"),
            ParseError::UnknownToken { field, token } => {
                write!(f, "unknown {field} value {token:?}")
            }
            ParseError::BadSlice {
                name,
                offset,
                length,
                blob_len,
            } => write!(
                f,
                "input {name:?} slice {offset}..{} exceeds bin length {blob_len}",
                offset + length
            ),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ParseError::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for ParseError {
    fn from(e: serde_json::Error) -> Self {
        ParseError::Json(e)
    }
}

/// Resolving a corpus from a filesystem path or version string failed.
#[derive(Debug)]
pub enum ResolveError {
    /// An I/O error reading the corpus artifact.
    Io(std::io::Error),
    /// The artifact was found but could not be parsed.
    Parse(ParseError),
    /// `Corpus::from_version` was asked for a version that is not the bundled
    /// one. Remote fetching is a deliberate, not-yet-built seam.
    VersionUnavailable { requested: String, bundled: String },
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveError::Io(e) => write!(f, "reading corpus artifact: {e}"),
            ResolveError::Parse(e) => write!(f, "{e}"),
            ResolveError::VersionUnavailable { requested, bundled } => write!(
                f,
                "version {requested:?} is not available: remote fetching is not built yet \
                 (bundled version is {bundled:?})"
            ),
        }
    }
}

impl std::error::Error for ResolveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ResolveError::Io(e) => Some(e),
            ResolveError::Parse(e) => Some(e),
            ResolveError::VersionUnavailable { .. } => None,
        }
    }
}

impl From<std::io::Error> for ResolveError {
    fn from(e: std::io::Error) -> Self {
        ResolveError::Io(e)
    }
}

impl From<ParseError> for ResolveError {
    fn from(e: ParseError) -> Self {
        ResolveError::Parse(e)
    }
}
