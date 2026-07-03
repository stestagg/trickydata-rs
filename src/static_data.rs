//! Static data shapes emitted by `static_*` macros.

use crate::DecodeAs;

/// A compile-time example emitted by [`crate::static_examples`].
///
/// `T` is the literal value type for the selected target: for example
/// `&'static str` for `String`, `&'static [u8]` for `Vec<u8>`, and the numeric
/// type itself for numeric targets. Deliberately-invalid would-be examples are
/// represented with `value: None` and their raw bytes still available.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StaticExample<T: 'static> {
    pub name: &'static str,
    pub path: &'static str,
    pub description: &'static str,
    pub tags: &'static [&'static str],
    pub decode_as: DecodeAs,
    pub invalid_as: Option<DecodeAs>,
    pub value: Option<T>,
    pub raw: &'static [u8],
}
