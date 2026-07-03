//! String sizing: pad tricky strings to an exact count of a Unicode unit.
//!
//! Use case: "give me tricky strings that are exactly 12 extended grapheme
//! clusters long". Each candidate whose count is `<=` the target is
//! left-padded with a filler character, then **re-measured** and kept only if
//! it lands exactly on the target — the re-measure guards the rare case where
//! the filler interacts with the string's leading character (e.g. a leading
//! combining mark).

use crate::decode_as::DecodeAs;
use crate::example::Example;
use crate::native::encode_text;
use crate::target::Target;

/// A Unicode unit a string can be sized in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeUnit {
    /// Encoded byte length.
    ByteLength,
    /// Encoding code units (bytes for utf8/ascii/latin1, 16-bit for utf16, …).
    CodeUnits,
    /// Unicode code points.
    CodePoints,
    /// Unicode scalar values (code points excluding surrogates).
    ScalarValues,
    /// Extended grapheme clusters (UAX #29). Requires the `sizing` feature.
    ExtendedGraphemes,
}

/// Bytes per encoding code unit.
fn code_unit_width(decode_as: DecodeAs) -> usize {
    match decode_as {
        DecodeAs::Utf16Le => 2,
        DecodeAs::Utf32Le => 4,
        _ => 1,
    }
}

/// Measure a string in `unit` under the given text encoding. `None` if the
/// string is not representable in that encoding.
fn measure(unit: SizeUnit, s: &str, decode_as: DecodeAs) -> Option<usize> {
    match unit {
        SizeUnit::ByteLength => encode_text(decode_as, s).map(|b| b.len()),
        SizeUnit::CodeUnits => {
            encode_text(decode_as, s).map(|b| b.len() / code_unit_width(decode_as))
        }
        SizeUnit::CodePoints => Some(s.chars().count()),
        // A Rust `&str` can never contain a surrogate, so every code point is a
        // scalar value.
        SizeUnit::ScalarValues => Some(s.chars().count()),
        SizeUnit::ExtendedGraphemes => grapheme_count(s),
    }
}

#[cfg(feature = "sizing")]
fn grapheme_count(s: &str) -> Option<usize> {
    use unicode_segmentation::UnicodeSegmentation;
    Some(s.graphemes(true).count())
}

#[cfg(not(feature = "sizing"))]
fn grapheme_count(_s: &str) -> Option<usize> {
    panic!(
        "extended-grapheme sizing needs the 'sizing' crate feature; \
         build trickydata with `--features sizing`"
    );
}

/// Return derived examples left-padded to exactly `target` of `unit`.
pub(crate) fn size_examples<T: Target>(
    examples: Vec<Example<T>>,
    unit: SizeUnit,
    target: usize,
    pad: char,
) -> Vec<Example<T>> {
    let pad_str = pad.to_string();
    let mut out = Vec::new();

    for ex in &examples {
        if !ex.is_text() {
            continue;
        }
        let Ok(s) = ex.as_text() else { continue };
        let decode_as = ex.decode_as();

        let Some(pad_contrib) = measure(unit, &pad_str, decode_as) else {
            continue;
        };
        if pad_contrib == 0 {
            continue;
        }
        let Some(count) = measure(unit, &s, decode_as) else {
            continue;
        };
        if count > target {
            continue;
        }
        let deficit = target - count;
        if deficit % pad_contrib != 0 {
            continue;
        }
        let padded: String = pad_str.repeat(deficit / pad_contrib) + &s;

        // The verification step: re-measure the assembled string.
        if measure(unit, &padded, decode_as) != Some(target) {
            continue;
        }
        let Some(raw) = encode_text(decode_as, &padded) else {
            continue;
        };

        out.push(Example::derived(
            format!("{}__padded", ex.name()),
            decode_as,
            raw,
            ex.description().to_string(),
            ex.tags().to_vec(),
            None, // recomputed counts would go here; sized strings drop the source meta
        ));
    }
    out
}
