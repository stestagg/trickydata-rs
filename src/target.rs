//! The [`Target`] trait: which Rust type a query produces, and the lossless
//! promotion lattice that decides corpus membership.
//!
//! The client is generic over the concrete target type and admits only
//! *losslessly promotable* inputs:
//!
//! * integer targets accept narrower, sign-compatible integers (the
//!   widening subset of [`From`]: `u8 -> u16`, `u8 -> i16`, …) — never
//!   narrowing, never a cross-sign conversion that loses range;
//! * float targets accept narrower floats (`f16 -> f32 -> f64`);
//! * **integers and floats never mix**, even where a value would be exact;
//! * [`String`] unifies every text encoding;
//! * [`Vec<u8>`] is the universal sink — every input has a byte payload.
//!
//! With `promote = false` a target narrows to its single native `decode-as`.

use crate::decode_as::DecodeAs;
use crate::native::Native;

/// A Rust type a [`crate::Corpus`] query can produce.
///
/// Implemented for `u8`..`u128`, `i8`..`i128`, `f32`, `f64`, `bool`, [`String`],
/// and `Vec<u8>` (and, with the `half` feature, [`half::f16`]).
pub trait Target: Sized {
    /// Whether string sizing applies to this target. Only [`String`] padding is
    /// meaningful; querying `.size(..)` on any other target is a usage error.
    const SUPPORTS_SIZING: bool = false;

    /// Whether an input with this `decode-as` is a member of this target's
    /// selection. With `promote` off, only the exact native token matches; with
    /// it on, the full lossless-promotion set matches.
    fn accepts(src: DecodeAs, promote: bool) -> bool;

    /// Produce the target value from an already-decoded [`Native`] and the raw
    /// payload. Returns `None` only if called on a value the lattice should have
    /// excluded (an internal invariant), so callers gate with [`Target::accepts`]
    /// first. The raw payload is supplied for the [`Vec<u8>`] sink.
    fn extract(native: &Native, raw: &[u8]) -> Option<Self>;
}

macro_rules! uint_target {
    ($t:ty, $native_tok:path, [$($src:path),* $(,)?]) => {
        impl Target for $t {
            fn accepts(src: DecodeAs, promote: bool) -> bool {
                if promote {
                    matches!(src, $($src)|*)
                } else {
                    src == $native_tok
                }
            }
            fn extract(n: &Native, _raw: &[u8]) -> Option<$t> {
                match n {
                    Native::U128(v) => <$t>::try_from(*v).ok(),
                    _ => None,
                }
            }
        }
    };
}

macro_rules! int_target {
    ($t:ty, $native_tok:path, [$($src:path),* $(,)?]) => {
        impl Target for $t {
            fn accepts(src: DecodeAs, promote: bool) -> bool {
                if promote {
                    matches!(src, $($src)|*)
                } else {
                    src == $native_tok
                }
            }
            fn extract(n: &Native, _raw: &[u8]) -> Option<$t> {
                match n {
                    Native::U128(v) => <$t>::try_from(*v).ok(),
                    Native::I128(v) => <$t>::try_from(*v).ok(),
                    _ => None,
                }
            }
        }
    };
}

use DecodeAs::*;

uint_target!(u8, U8, [U8]);
uint_target!(u16, U16Le, [U8, U16Le]);
uint_target!(u32, U32Le, [U8, U16Le, U32Le]);
uint_target!(u64, U64Le, [U8, U16Le, U32Le, U64Le]);
uint_target!(u128, U128Le, [U8, U16Le, U32Le, U64Le, U128Le]);

int_target!(i8, I8, [I8]);
int_target!(i16, I16Le, [I8, I16Le, U8]);
int_target!(i32, I32Le, [I8, I16Le, I32Le, U8, U16Le]);
int_target!(i64, I64Le, [I8, I16Le, I32Le, I64Le, U8, U16Le, U32Le]);
int_target!(
    i128,
    I128Le,
    [I8, I16Le, I32Le, I64Le, I128Le, U8, U16Le, U32Le, U64Le]
);

impl Target for f32 {
    fn accepts(src: DecodeAs, promote: bool) -> bool {
        if promote {
            matches!(src, F16Le | F32Le)
        } else {
            src == F32Le
        }
    }
    fn extract(n: &Native, _raw: &[u8]) -> Option<f32> {
        // Every admitted source (f16, f32) is exactly representable in f32; the
        // f64 carrier holds the widened value, so narrowing back is lossless.
        match n {
            Native::F64(v) => Some(*v as f32),
            _ => None,
        }
    }
}

impl Target for f64 {
    fn accepts(src: DecodeAs, promote: bool) -> bool {
        if promote {
            matches!(src, F16Le | F32Le | F64Le)
        } else {
            src == F64Le
        }
    }
    fn extract(n: &Native, _raw: &[u8]) -> Option<f64> {
        match n {
            Native::F64(v) => Some(*v),
            _ => None,
        }
    }
}

impl Target for bool {
    fn accepts(src: DecodeAs, _promote: bool) -> bool {
        src == Bool
    }
    fn extract(n: &Native, _raw: &[u8]) -> Option<bool> {
        match n {
            Native::Bool(v) => Some(*v),
            _ => None,
        }
    }
}

impl Target for String {
    const SUPPORTS_SIZING: bool = true;

    fn accepts(src: DecodeAs, _promote: bool) -> bool {
        // All text encodings collapse into one Rust String. Promotion does not
        // widen strings (bytes -> str is not lossless).
        src.is_text()
    }
    fn extract(n: &Native, _raw: &[u8]) -> Option<String> {
        match n {
            Native::Text(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl Target for Vec<u8> {
    fn accepts(src: DecodeAs, promote: bool) -> bool {
        // Native set is just `bytes`; with promotion the sink swallows every
        // input (each has a raw payload).
        promote || src == Bytes
    }
    fn extract(_n: &Native, raw: &[u8]) -> Option<Vec<u8>> {
        Some(raw.to_vec())
    }
}

#[cfg(feature = "half")]
impl Target for half::f16 {
    fn accepts(src: DecodeAs, _promote: bool) -> bool {
        // f16 has nothing narrower to promote from; it only ever matches f16.
        src == F16Le
    }
    fn extract(n: &Native, _raw: &[u8]) -> Option<half::f16> {
        // The carrier holds the f16 value widened to f64; narrowing back is
        // exact because every f16 is representable in f64.
        match n {
            Native::F64(v) => Some(half::f16::from_f64(*v)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The macro source-lists are the only hand-written copy of the lattice.
    /// Assert each admitted `(source -> target)` pair is a real, total
    /// lossless conversion, so the table can never silently drift into a lossy
    /// or out-of-range promotion. Spot-check representative widths.
    #[test]
    fn promotion_is_lossless_widening() {
        // u8 promotes into every wider int (both signs) but not u8->i8 (range).
        assert!(<i16 as Target>::accepts(U8, true));
        assert!(<i128 as Target>::accepts(U8, true));
        assert!(!<i8 as Target>::accepts(U8, true)); // 255 doesn't fit i8
        assert!(!<u8 as Target>::accepts(U16Le, true)); // narrowing forbidden
                                                      // Cross-sign that loses range is forbidden.
        assert!(!<i32 as Target>::accepts(U32Le, true)); // u32::MAX > i32::MAX
        assert!(<i64 as Target>::accepts(U32Le, true)); // but fits i64
                                                      // Ints and floats never mix.
        assert!(!<f32 as Target>::accepts(U8, true));
        assert!(!<f64 as Target>::accepts(U32Le, true));
        assert!(!<i32 as Target>::accepts(F32Le, true));
        // Float widening only.
        assert!(<f32 as Target>::accepts(F16Le, true));
        assert!(<f64 as Target>::accepts(F32Le, true));
        assert!(!<f32 as Target>::accepts(F64Le, true)); // narrowing forbidden
    }

    #[test]
    fn promote_off_is_native_only() {
        assert!(<i32 as Target>::accepts(I32Le, false));
        assert!(!<i32 as Target>::accepts(I8, false));
        assert!(!<i32 as Target>::accepts(U8, false));
        // The bytes sink narrows to native `bytes` when promotion is off.
        assert!(<Vec<u8> as Target>::accepts(Bytes, false));
        assert!(!<Vec<u8> as Target>::accepts(Utf8, false));
        assert!(<Vec<u8> as Target>::accepts(Utf8, true));
    }

    #[test]
    fn extract_narrows_carrier() {
        assert_eq!(<u16 as Target>::extract(&Native::U128(255), &[]), Some(255));
        assert_eq!(<i16 as Target>::extract(&Native::U128(255), &[]), Some(255));
        assert_eq!(<i16 as Target>::extract(&Native::I128(-1), &[]), Some(-1));
        // Out of range yields None (belt-and-suspenders; accepts() gates first).
        assert_eq!(<u8 as Target>::extract(&Native::U128(256), &[]), None);
    }
}
