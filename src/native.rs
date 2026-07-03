//! Decode a payload into its natural ("native") Rust value.
//!
//! Numerics are little-endian with no padding; text is decoded per its
//! encoding into a Rust `String`. Integers widen into a 128-bit carrier (one
//! variant per signedness instead of one per width); floats widen into `f64`.
//! The *original* [`DecodeAs`] token — not the carrier — is what target
//! membership is tested against, so no precision claim is implied by the
//! widening.

use crate::decode_as::DecodeAs;
use crate::error::DecodeError;

/// A decoded value, tagged by signedness/kind rather than exact width.
#[derive(Clone, Debug, PartialEq)]
pub enum Native {
    /// Any unsigned integer (`u8`..`u128`), widened to `u128`.
    U128(u128),
    /// Any signed integer (`i8`..`i128`), widened to `i128`.
    I128(i128),
    /// Any float (`f16`/`f32`/`f64`), widened losslessly to `f64`.
    F64(f64),
    Bool(bool),
    /// Any text encoding, decoded to a Rust `String`.
    Text(String),
    /// Raw bytes (`decode-as: bytes`).
    Bytes(Vec<u8>),
}

/// Decode a payload as its declared `decode_as`.
///
/// Errors when the payload is the wrong width for a numeric type or is invalid
/// in its text encoding — exactly the deliberately-invalid cases the corpus
/// stores as `decode-as: bytes` with an `invalid-as` token.
pub fn decode_native(decode_as: DecodeAs, payload: &[u8]) -> Result<Native, DecodeError> {
    use DecodeAs::*;
    match decode_as {
        U8 | U16Le | U32Le | U64Le | U128Le => Ok(Native::U128(decode_uint(decode_as, payload)?)),
        I8 | I16Le | I32Le | I64Le | I128Le => Ok(Native::I128(decode_int(decode_as, payload)?)),
        F16Le => Ok(Native::F64(
            f16_bits_to_f32(decode_u16(decode_as, payload)?) as f64,
        )),
        F32Le => {
            let bits = decode_u32(decode_as, payload)? as u32;
            Ok(Native::F64(f32::from_bits(bits) as f64))
        }
        F64Le => {
            let bits = decode_u64(decode_as, payload)? as u64;
            Ok(Native::F64(f64::from_bits(bits)))
        }
        Bool => Ok(Native::Bool(payload.iter().any(|&b| b != 0))),
        Bytes => Ok(Native::Bytes(payload.to_vec())),
        Ascii | Utf8 | Utf16Le | Utf32Le | Latin1 => {
            Ok(Native::Text(decode_text(decode_as, payload)?))
        }
    }
}

/// Byte width of a fixed-size numeric `decode-as`.
fn numeric_width(decode_as: DecodeAs) -> usize {
    use DecodeAs::*;
    match decode_as {
        U8 | I8 => 1,
        U16Le | I16Le | F16Le => 2,
        U32Le | I32Le | F32Le => 4,
        U64Le | I64Le | F64Le => 8,
        U128Le | I128Le => 16,
        _ => unreachable!("numeric_width called on non-numeric decode-as"),
    }
}

fn require_width(decode_as: DecodeAs, payload: &[u8]) -> Result<usize, DecodeError> {
    let expected = numeric_width(decode_as);
    if payload.len() != expected {
        return Err(DecodeError::WrongLength {
            decode_as,
            expected,
            got: payload.len(),
        });
    }
    Ok(expected)
}

fn decode_uint(decode_as: DecodeAs, payload: &[u8]) -> Result<u128, DecodeError> {
    require_width(decode_as, payload)?;
    let mut bytes = [0u8; 16];
    bytes[..payload.len()].copy_from_slice(payload); // little-endian zero-extend
    Ok(u128::from_le_bytes(bytes))
}

fn decode_int(decode_as: DecodeAs, payload: &[u8]) -> Result<i128, DecodeError> {
    let width = require_width(decode_as, payload)?;
    let mut bytes = [0u8; 16];
    bytes[..width].copy_from_slice(payload);
    // Sign-extend: if the top bit of the most-significant input byte is set,
    // fill the remaining high bytes with 0xFF.
    if width < 16 && payload[width - 1] & 0x80 != 0 {
        for b in &mut bytes[width..] {
            *b = 0xFF;
        }
    }
    Ok(i128::from_le_bytes(bytes))
}

fn decode_u16(decode_as: DecodeAs, payload: &[u8]) -> Result<u16, DecodeError> {
    require_width(decode_as, payload)?;
    Ok(u16::from_le_bytes([payload[0], payload[1]]))
}

fn decode_u32(decode_as: DecodeAs, payload: &[u8]) -> Result<u128, DecodeError> {
    require_width(decode_as, payload)?;
    Ok(u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as u128)
}

fn decode_u64(decode_as: DecodeAs, payload: &[u8]) -> Result<u128, DecodeError> {
    require_width(decode_as, payload)?;
    let mut b = [0u8; 8];
    b.copy_from_slice(payload);
    Ok(u64::from_le_bytes(b) as u128)
}

/// Expand an IEEE-754 binary16 (stored as raw bits) into an `f32`. This is
/// exact: every `f16` value is representable in `f32`.
fn f16_bits_to_f32(bits: u16) -> f32 {
    let sign = (bits >> 15) & 0x1;
    let exp = (bits >> 10) & 0x1F;
    let mant = bits & 0x3FF;

    let f32_bits: u32 = match exp {
        // Zero / subnormal.
        0 => {
            if mant == 0 {
                (sign as u32) << 31
            } else {
                // An f16 subnormal has value `mant * 2^-24`. Renormalize it as a
                // (normal) f32: the leading set bit at position `p` becomes the
                // implicit 1, giving exponent `p - 24`, and the remaining bits
                // are the fraction.
                let p = (15 - mant.leading_zeros()) as i32; // 0..=9
                let frac = (mant ^ (1 << p)) as u32; // clear the leading bit
                let f32_exp = (p - 24 + 127) as u32;
                ((sign as u32) << 31) | (f32_exp << 23) | (frac << (23 - p as u32))
            }
        }
        // Inf / NaN.
        0x1F => ((sign as u32) << 31) | (0xFF << 23) | ((mant as u32) << 13),
        // Normal.
        _ => {
            let f32_exp = (exp as i32 - 15 + 127) as u32;
            ((sign as u32) << 31) | (f32_exp << 23) | ((mant as u32) << 13)
        }
    };
    f32::from_bits(f32_bits)
}

fn decode_text(decode_as: DecodeAs, payload: &[u8]) -> Result<String, DecodeError> {
    use DecodeAs::*;
    match decode_as {
        Ascii => {
            if let Some(pos) = payload.iter().position(|&b| b >= 0x80) {
                return Err(DecodeError::InvalidText {
                    decode_as,
                    detail: format!("non-ASCII byte 0x{:02x} at index {pos}", payload[pos]),
                });
            }
            // Validated ASCII is valid UTF-8.
            Ok(String::from_utf8(payload.to_vec()).expect("ascii is valid utf8"))
        }
        Utf8 => std::str::from_utf8(payload)
            .map(str::to_owned)
            .map_err(|e| DecodeError::InvalidText {
                decode_as,
                detail: e.to_string(),
            }),
        // Latin-1: every byte is a code point in 0..=255.
        Latin1 => Ok(payload.iter().map(|&b| b as char).collect()),
        Utf16Le => {
            if payload.len() % 2 != 0 {
                return Err(DecodeError::InvalidText {
                    decode_as,
                    detail: format!("byte length {} is not a multiple of 2", payload.len()),
                });
            }
            let units: Vec<u16> = payload
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            char::decode_utf16(units)
                .collect::<Result<String, _>>()
                .map_err(|e| DecodeError::InvalidText {
                    decode_as,
                    detail: format!("unpaired surrogate 0x{:04x}", e.unpaired_surrogate()),
                })
        }
        Utf32Le => {
            if payload.len() % 4 != 0 {
                return Err(DecodeError::InvalidText {
                    decode_as,
                    detail: format!("byte length {} is not a multiple of 4", payload.len()),
                });
            }
            payload
                .chunks_exact(4)
                .map(|c| {
                    let v = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
                    char::from_u32(v).ok_or_else(|| DecodeError::InvalidText {
                        decode_as,
                        detail: format!("0x{v:08x} is not a Unicode scalar value"),
                    })
                })
                .collect()
        }
        _ => unreachable!("decode_text called on non-text decode-as"),
    }
}

/// Encode a string back into the bytes of a text encoding. Used by string
/// sizing to materialise a padded string as its proper payload. `None` if a
/// character is unrepresentable in the target encoding (e.g. non-ASCII in
/// `ascii`, code point > 255 in `latin1`).
pub fn encode_text(decode_as: DecodeAs, s: &str) -> Option<Vec<u8>> {
    use DecodeAs::*;
    match decode_as {
        Utf8 => Some(s.as_bytes().to_vec()),
        Ascii => {
            if s.is_ascii() {
                Some(s.as_bytes().to_vec())
            } else {
                None
            }
        }
        Latin1 => s
            .chars()
            .map(|c| (c as u32 <= 0xFF).then_some(c as u8))
            .collect(),
        Utf16Le => {
            let mut out = Vec::new();
            for unit in s.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            Some(out)
        }
        Utf32Le => {
            let mut out = Vec::with_capacity(s.chars().count() * 4);
            for c in s.chars() {
                out.extend_from_slice(&(c as u32).to_le_bytes());
            }
            Some(out)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_one_is_exact() {
        // 1.0 in binary16 is 0x3C00.
        assert_eq!(f16_bits_to_f32(0x3C00), 1.0);
    }

    #[test]
    fn f16_round_trips_known_values() {
        assert_eq!(f16_bits_to_f32(0x0000), 0.0);
        assert_eq!(f16_bits_to_f32(0x8000), -0.0);
        assert_eq!(f16_bits_to_f32(0x4000), 2.0);
        assert_eq!(f16_bits_to_f32(0xC000), -2.0);
        assert!(f16_bits_to_f32(0x7C00).is_infinite());
        assert!(f16_bits_to_f32(0x7E00).is_nan());
        // Smallest positive subnormal: 2^-24.
        assert_eq!(f16_bits_to_f32(0x0001), 2f32.powi(-24));
    }

    #[test]
    fn signed_min_values_decode() {
        assert_eq!(decode_int(DecodeAs::I8, &[0x80]).unwrap(), -128);
        assert_eq!(
            decode_int(DecodeAs::I16Le, &0x8000u16.to_le_bytes()).unwrap(),
            -32768
        );
    }

    #[test]
    fn unsigned_max_values_decode() {
        assert_eq!(decode_uint(DecodeAs::U8, &[0xFF]).unwrap(), 255);
        assert_eq!(decode_uint(DecodeAs::U128Le, &[0xFF; 16]).unwrap(), u128::MAX);
    }

    #[test]
    fn wrong_length_errors() {
        assert!(matches!(
            decode_native(DecodeAs::U32Le, &[0, 1]),
            Err(DecodeError::WrongLength { .. })
        ));
    }

    #[test]
    fn invalid_utf8_errors() {
        assert!(matches!(
            decode_native(DecodeAs::Utf8, &[0xFF, 0xFE]),
            Err(DecodeError::InvalidText { .. })
        ));
    }
}
