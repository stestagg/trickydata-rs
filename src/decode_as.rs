//! The `decode-as` token: the logical type a corpus input is interpreted as.
//!
//! This mirrors the `decode-as` enum in `frontmatter-schema.yaml`. The compiler
//! treats `decode-as` as opaque passthrough metadata (a `String`); the client
//! parses it into this enum so membership and decoding can match on a closed set.

use std::fmt;
use std::str::FromStr;

/// A logical type an input may decode as. Covers fixed-width Rust numerics and
/// the string/byte encodings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecodeAs {
    U8,
    U16Le,
    U32Le,
    U64Le,
    U128Le,
    I8,
    I16Le,
    I32Le,
    I64Le,
    I128Le,
    F16Le,
    F32Le,
    F64Le,
    Bool,
    Bytes,
    Ascii,
    Utf8,
    Utf16Le,
    Utf32Le,
    Latin1,
}

impl DecodeAs {
    /// The canonical token string (matches the schema enum and the index JSON).
    pub fn as_str(self) -> &'static str {
        use DecodeAs::*;
        match self {
            U8 => "u8",
            U16Le => "u16-le",
            U32Le => "u32-le",
            U64Le => "u64-le",
            U128Le => "u128-le",
            I8 => "i8",
            I16Le => "i16-le",
            I32Le => "i32-le",
            I64Le => "i64-le",
            I128Le => "i128-le",
            F16Le => "f16-le",
            F32Le => "f32-le",
            F64Le => "f64-le",
            Bool => "bool",
            Bytes => "bytes",
            Ascii => "ascii",
            Utf8 => "utf8",
            Utf16Le => "utf16-le",
            Utf32Le => "utf32-le",
            Latin1 => "latin1",
        }
    }

    /// True for the string/byte text encodings (everything that decodes to a
    /// Rust `String`). `bytes` is *not* text.
    pub fn is_text(self) -> bool {
        use DecodeAs::*;
        matches!(self, Ascii | Utf8 | Utf16Le | Utf32Le | Latin1)
    }
}

impl fmt::Display for DecodeAs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DecodeAs {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        use DecodeAs::*;
        Ok(match s {
            "u8" => U8,
            "u16-le" => U16Le,
            "u32-le" => U32Le,
            "u64-le" => U64Le,
            "u128-le" => U128Le,
            "i8" => I8,
            "i16-le" => I16Le,
            "i32-le" => I32Le,
            "i64-le" => I64Le,
            "i128-le" => I128Le,
            "f16-le" => F16Le,
            "f32-le" => F32Le,
            "f64-le" => F64Le,
            "bool" => Bool,
            "bytes" => Bytes,
            "ascii" => Ascii,
            "utf8" => Utf8,
            "utf16-le" => Utf16Le,
            "utf32-le" => Utf32Le,
            "latin1" => Latin1,
            _ => return Err(()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::DecodeAs;

    #[test]
    fn little_endian_tokens_are_canonical() {
        assert_eq!(DecodeAs::U16Le.as_str(), "u16-le");
        assert_eq!("u16-le".parse::<DecodeAs>(), Ok(DecodeAs::U16Le));
        assert!("u16".parse::<DecodeAs>().is_err());

        assert_eq!(DecodeAs::F32Le.as_str(), "f32-le");
        assert_eq!("f32-le".parse::<DecodeAs>(), Ok(DecodeAs::F32Le));
        assert!("f32".parse::<DecodeAs>().is_err());

        assert_eq!(DecodeAs::Utf16Le.as_str(), "utf16-le");
        assert_eq!("utf16-le".parse::<DecodeAs>(), Ok(DecodeAs::Utf16Le));
        assert!("utf16".parse::<DecodeAs>().is_err());
    }
}
