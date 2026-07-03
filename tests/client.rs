//! Behavioral tests for the trickydata Rust client against the bundled corpus.

use std::collections::HashSet;
use std::path::Path;

use trickydata::{Corpus, DecodeAs, Example, PairHint, SizeUnit, Sort, Target};

fn corpus() -> Corpus {
    Corpus::bundled()
}

fn names<T: Target>(examples: &[Example<T>]) -> HashSet<String> {
    examples.iter().map(|e| e.name().to_string()).collect()
}

// --- membership & lossless promotion ------------------------------------------

#[test]
fn integer_promotion_is_lossless_widening() {
    let c = corpus();

    // Native u8 inputs (promotion off) are a strict subset of the u16 view,
    // which also pulls in the u8 inputs via lossless widening.
    let native_u8 = names(&c.get::<u8>().promote(false).examples());
    let as_u16 = names(&c.get::<u16>().examples());
    assert!(!native_u8.is_empty());
    assert!(native_u8.is_subset(&as_u16));
    assert!(native_u8.len() < as_u16.len());

    // Every input admitted into i32 has a sign-compatible, narrower-or-equal type.
    let allowed = [
        DecodeAs::I8,
        DecodeAs::I16Le,
        DecodeAs::I32Le,
        DecodeAs::U8,
        DecodeAs::U16Le,
    ];
    let i32_examples = c.get::<i32>().examples();
    assert!(!i32_examples.is_empty());
    assert!(i32_examples
        .iter()
        .all(|e| allowed.contains(&e.decode_as())));
    // ...and it genuinely includes promoted (non-native) inputs.
    assert!(i32_examples.iter().any(|e| e.decode_as() != DecodeAs::I32Le));
}

#[test]
fn ints_and_floats_never_mix() {
    let c = corpus();
    // A u32 input must not appear in any float target (ints/floats stay separate).
    let f64_names = names(&c.get::<f64>().examples());
    let f32_names = names(&c.get::<f32>().examples());
    assert!(f64_names.contains("float_f64_max_finite") || !f64_names.is_empty());
    assert!(!f64_names.contains("integer_u32_above_u16_maximum"));
    assert!(!f32_names.contains("integer_u32_above_u16_maximum"));

    // Conversely, no float input leaks into an integer target.
    let i128_examples = c.get::<i128>().examples();
    assert!(i128_examples
        .iter()
        .all(|e| !matches!(e.decode_as(), DecodeAs::F16Le | DecodeAs::F32Le | DecodeAs::F64Le)));
}

#[test]
fn values_are_homogeneous_and_typed() {
    let c = corpus();
    assert!(!c.get::<String>().values().is_empty());
    assert!(!c.get::<f64>().values().is_empty());
    assert!(!c.get::<u128>().values().is_empty());
    assert!(!c.get::<Vec<u8>>().values().is_empty());
}

// --- return modes -------------------------------------------------------------

#[test]
fn three_return_modes() {
    let c = corpus();
    let examples = c.get::<String>().tags(["sql"]).examples();
    assert!(!examples.is_empty());
    let raw = c.get::<String>().tags(["sql"]).raw();
    assert!(!raw.is_empty());
    // raw includes invalid sql (decode-as bytes, invalid-as utf8); the value
    // view excludes it, so raw >= values.
    let values = c.get::<String>().tags(["sql"]).values();
    assert!(raw.len() >= values.len());
}

// --- bytes universal sink -----------------------------------------------------

#[test]
fn bytes_is_universal_sink() {
    let c = corpus();
    // Valid text SQL strings (exclude the deliberately-invalid one).
    let sql_strings = names(
        &c.get::<String>()
            .tags(["sql"])
            .include_invalid(false)
            .examples(),
    );
    let sql_bytes = names(&c.get::<Vec<u8>>().tags(["sql"]).examples());
    assert!(!sql_strings.is_empty());
    // The text SQL strings also surface under the bytes sink...
    assert!(sql_strings.is_subset(&sql_bytes));
    // ...and disappear when promotion (the sink) is turned off.
    let sql_bytes_native = names(&c.get::<Vec<u8>>().tags(["sql"]).promote(false).examples());
    assert!(sql_strings.is_disjoint(&sql_bytes_native));
}

// --- floats unify -------------------------------------------------------------

#[test]
fn floats_unify_widths() {
    let c = corpus();
    let examples = c.get::<f64>().examples();
    let decode_set: HashSet<DecodeAs> = examples.iter().map(|e| e.decode_as()).collect();
    assert!(decode_set.contains(&DecodeAs::F16Le));
    assert!(decode_set.contains(&DecodeAs::F32Le));
    assert!(decode_set.contains(&DecodeAs::F64Le));

    // An f32 input read as f64 keeps its exact value (1.0).
    let one = examples
        .iter()
        .find(|e| e.name() == "float_f32_one")
        .expect("float_f32_one present");
    assert_eq!(one.value().unwrap(), 1.0_f64);
}

// --- integer boundaries -------------------------------------------------------

#[test]
fn integer_boundaries_decode() {
    let c = corpus();
    // i128 accepts every signed+unsigned narrower int, so it is a convenient
    // lens for reading boundary values across widths.
    let signed: std::collections::HashMap<String, i128> = c
        .get::<i128>()
        .examples()
        .iter()
        .map(|e| (e.name().to_string(), e.value().unwrap()))
        .collect();
    assert_eq!(signed["integer_i8_minimum"], -128);
    assert_eq!(signed["integer_i16_minimum"], -32768);
    assert_eq!(signed["integer_i32_minimum"], -(1i128 << 31));
    assert_eq!(signed["integer_i64_minimum"], -(1i128 << 63));
    assert_eq!(signed["integer_u8_maximum"], 255);
    assert_eq!(signed["integer_u64_maximum"], (1i128 << 64) - 1);

    // The 128-bit extremes need their own-width targets.
    let i128_min = c.get::<i128>().names(["integer_i128_minimum"]).values()[0];
    assert_eq!(i128_min, i128::MIN);
    let u128_max = c.get::<u128>().names(["integer_u128_maximum"]).values()[0];
    assert_eq!(u128_max, u128::MAX);
}

// --- deliberately-invalid inputs ----------------------------------------------

#[test]
fn invalid_excluded_from_values_but_present_as_examples() {
    let c = corpus();
    // The invalid utf8 SQL input is a member of the String selection...
    let example_names = names(&c.get::<String>().examples());
    assert!(example_names.contains("language_sql_data_invalid_utf8"));
    // ...but contributes no value (values stays homogeneous and decodable).
    let values = c.get::<String>().values();
    assert!(!values.is_empty());
}

#[test]
fn invalid_value_is_honest() {
    let c = corpus();
    let examples = c.get::<String>().tags(["invalid-utf8"]).examples();
    assert!(!examples.is_empty());
    for e in &examples {
        assert_eq!(e.invalid_as(), Some(DecodeAs::Utf8));
        assert!(!e.is_valid());
        assert!(!e.is_text()); // stored as bytes
        assert!(e.value().is_err()); // value() attempts the real utf8 decode
        assert!(e.as_text().is_err()); // guards on the stored bytes type
        assert!(!e.as_bytes().is_empty()); // raw payload always reachable
    }
    // raw mode yields a homogeneous list that includes the invalid inputs.
    let raw = c.get::<String>().tags(["invalid-utf8"]).raw();
    assert_eq!(raw.len(), examples.len());
}

#[test]
fn include_invalid_toggle() {
    let c = corpus();
    let on = names(&c.get::<String>().tags(["invalid-utf8"]).examples());
    let off = names(
        &c.get::<String>()
            .tags(["invalid-utf8"])
            .include_invalid(false)
            .examples(),
    );
    assert!(on.contains("language_sql_data_invalid_utf8"));
    assert!(!off.contains("language_sql_data_invalid_utf8"));
}

#[test]
fn invalid_stays_native_to_bytes() {
    let c = corpus();
    let name = "language_sql_data_invalid_utf8";
    // decode-as bytes => an ordinary bytes member regardless of include_invalid.
    let on = names(&c.get::<Vec<u8>>().tags(["invalid-utf8"]).examples());
    let off = names(
        &c.get::<Vec<u8>>()
            .tags(["invalid-utf8"])
            .include_invalid(false)
            .examples(),
    );
    assert!(on.contains(name));
    assert!(off.contains(name));
    // Its value through the bytes view is just bytes — no decode attempted.
    let e = c
        .get::<Vec<u8>>()
        .names([name])
        .examples()
        .into_iter()
        .next()
        .unwrap();
    assert!(e.value().is_ok());
}

#[test]
fn invalid_is_general_not_utf8_specific() {
    let c = corpus();
    // Truncated f32s surface in the f32 target, carry invalid-as f32, and err
    // on decode — however many the corpus happens to hold.
    let f = c.get::<f32>().tags(["invalid-f32"]).examples();
    assert!(!f.is_empty());
    assert!(f
        .iter()
        .all(|e| e.invalid_as() == Some(DecodeAs::F32Le) && e.value().is_err()));
    // ...and they all vanish when invalid inputs are excluded.
    let valid_only = c
        .get::<f32>()
        .tags(["invalid-f32"])
        .include_invalid(false)
        .examples();
    assert!(valid_only.is_empty());

    // A lone UTF-16 surrogate surfaces in the String target the same way.
    let s = c.get::<String>().tags(["invalid-utf16"]).examples();
    assert!(!s.is_empty());
    assert!(s
        .iter()
        .all(|e| e.invalid_as() == Some(DecodeAs::Utf16Le) && e.value().is_err()));
}

// --- pairs --------------------------------------------------------------------

#[test]
fn pairs_explicit_hint_only() {
    let c = corpus();
    let tricky = c.pairs::<f64>().hint(PairHint::Tricky).examples();
    assert!(!tricky.is_empty());
    assert!(tricky
        .iter()
        .all(|p| p.related && p.hint == Some(PairHint::Tricky)));
}

#[test]
fn pairs_value_tuples() {
    let c = corpus();
    let pairs = c.pairs::<f64>().hint(PairHint::Tricky).values();
    assert!(!pairs.is_empty());
}

#[test]
fn pairs_include_unrelated_and_deterministic() {
    let c = corpus();
    let a = c.pairs::<f64>().seed(0).max(30).examples();
    let b = c.pairs::<f64>().seed(0).max(30).examples();
    let keys = |ps: &[trickydata::Pair<f64>]| -> Vec<(String, String)> {
        ps.iter()
            .map(|p| (p.a.name().to_string(), p.b.name().to_string()))
            .collect()
    };
    assert_eq!(keys(&a), keys(&b)); // reproducible for a fixed seed
    assert!(a.iter().any(|p| !p.related)); // some unrelated pairs drawn
    assert!(a.len() <= 30);
}

#[test]
fn pair_destructures() {
    let c = corpus();
    let p = c
        .pairs::<f64>()
        .hint(PairHint::Tricky)
        .examples()
        .into_iter()
        .next()
        .unwrap();
    let collected: Vec<_> = p.into_iter().collect();
    assert_eq!(collected.len(), 2);
}

// --- string sizing ------------------------------------------------------------

#[test]
fn sizing_code_points_no_feature_needed() {
    let c = corpus();
    let out = c.get::<String>().size(SizeUnit::CodePoints, 8).examples();
    assert!(!out.is_empty());
    for e in &out {
        assert_eq!(e.as_text().unwrap().chars().count(), 8);
    }
}

#[cfg(feature = "sizing")]
#[test]
fn sizing_extended_graphemes() {
    use unicode_segmentation::UnicodeSegmentation;
    let c = corpus();
    let out = c
        .get::<String>()
        .size(SizeUnit::ExtendedGraphemes, 12)
        .examples();
    assert!(!out.is_empty());
    for e in &out {
        assert_eq!(e.as_text().unwrap().graphemes(true).count(), 12);
    }
}

#[cfg(feature = "half")]
#[test]
fn half_f16_target() {
    let c = corpus();
    // The native f16 target accepts only f16 inputs (nothing narrower exists).
    let f16s = c.get::<half::f16>().examples();
    assert!(!f16s.is_empty());
    assert!(f16s.iter().all(|e| e.decode_as() == DecodeAs::F16Le));
    // f16 still promotes into the wider float targets.
    let f64_decode: HashSet<DecodeAs> = c
        .get::<f64>()
        .examples()
        .iter()
        .map(|e| e.decode_as())
        .collect();
    assert!(f64_decode.contains(&DecodeAs::F16Le));
    // A known f16 value round-trips.
    let one = c.get::<half::f16>().examples().into_iter().find(|e| {
        e.value()
            .map(|v| v == half::f16::from_f32(1.0))
            .unwrap_or(false)
    });
    assert!(one.is_some());
}

// --- corpus resolution --------------------------------------------------------

#[test]
fn version_resolution_seam() {
    let c = corpus();
    let v = c.version().to_string();
    assert!(Corpus::from_version(&v).is_ok());
    assert!(Corpus::from_version("latest").is_ok());
    assert!(Corpus::from_version("definitely-not-a-version").is_err());
}

#[test]
fn loads_local_trickydata_artifact() {
    let path = [".inputs/trickydata.trickydata", "../trickydata-inputs/trickydata.trickydata"]
        .into_iter()
        .find(|path| Path::new(path).is_file())
        .expect("expected a downloaded or sibling trickydata.trickydata artifact");
    let c = Corpus::from_path(path).unwrap();
    assert_eq!(c.version(), corpus().version());
    assert!(!c.inputs().is_empty());
}

#[test]
fn sort_modes() {
    let c = corpus();
    let sorted = c.get::<f64>().sort(Sort::Name).examples();
    let mut expected: Vec<String> = sorted.iter().map(|e| e.name().to_string()).collect();
    let actual = expected.clone();
    expected.sort();
    assert_eq!(actual, expected);
}
