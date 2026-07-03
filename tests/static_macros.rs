use trickydata::{DecodeAs, StaticExample};

static SQL_STRINGS: &[&str] =
    trickydata::static_values!(String, tags = ["sql"], include_invalid = false);
static FLOATS: &[f64] = trickydata::static_values!(f64);
static BAD_UTF8: &[StaticExample<&str>] =
    trickydata::static_examples!(String, tags = ["invalid-utf8"]);
static SQL_BYTES: &[StaticExample<&[u8]>] =
    trickydata::static_examples!(Vec<u8>, tags = ["sql"], promote = false);

#[test]
fn static_values_emit_string_literals() {
    assert!(!SQL_STRINGS.is_empty());
    assert!(SQL_STRINGS
        .iter()
        .any(|s| s.contains("'") || s.contains("--")));
}

#[test]
fn static_values_emit_float_literals() {
    assert!(!FLOATS.is_empty());
    assert!(FLOATS.iter().any(|v| *v == 1.0));
    assert!(FLOATS.iter().any(|v| v.is_nan()));
}

#[test]
fn static_examples_include_invalid_without_value() {
    assert!(!BAD_UTF8.is_empty());
    assert!(BAD_UTF8
        .iter()
        .any(|e| e.name == "language_sql_data_invalid_utf8"));
    assert!(BAD_UTF8.iter().all(|e| e.decode_as == DecodeAs::Bytes));
    assert!(BAD_UTF8
        .iter()
        .all(|e| e.invalid_as == Some(DecodeAs::Utf8)));
    assert!(BAD_UTF8.iter().all(|e| e.value.is_none()));
    assert!(BAD_UTF8.iter().all(|e| !e.raw.is_empty()));
}

#[test]
fn static_examples_respect_promote_false_for_bytes() {
    assert!(!SQL_BYTES.is_empty());
    assert!(SQL_BYTES.iter().all(|e| e.decode_as == DecodeAs::Bytes));
    assert!(SQL_BYTES.iter().all(|e| e.value.is_some()));
}
