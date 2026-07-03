# trickydata (Rust client)

A Rust client for the **trickydata** corpus — a curated set of deliberately
tricky test inputs (edge-case Unicode strings, IEEE-754 floats, boundary
integers, SQL edge cases, and binary blobs). Pull these inputs straight into your
own tests.

```rust
use trickydata::{Corpus, PairHint};

let c = Corpus::bundled();

// u8 + u16 inputs, every value losslessly a u16:
let v: Vec<u16> = c.get::<u16>().values();

// tricky float *parse strings*, minus the C-syntax ones:
let floaty: Vec<String> = c.get::<String>().tags(["float"]).exclude_tags(["iso-c"]).values();

// every IEEE-754 edge value as f64 (f16/f32/f64 promote in):
let floats: Vec<f64> = c.get::<f64>().values();

// SQL edge cases (decode-as ascii/utf8) live under `String`; select by tag:
let sql: Vec<String> = c.get::<String>().tags(["sql"]).values();

// the same inputs as raw bytes (Vec<u8> is the universal sink):
let sql_bytes: Vec<Vec<u8>> = c.get::<Vec<u8>>().tags(["sql"]).raw();
```

## Targets

This client is **generic over the target Rust type** and uses **lossless
promotion**: asking for a wider type pulls in any narrower input that converts
without loss.

| target | accepts (`decode-as`) | notes |
|--------|-----------------------|-------|
| `u8`…`u128` | unsigned ints ≤ the target's width | lossless widening |
| `i8`…`i128` | signed + unsigned ints that fit losslessly | e.g. `i32` ← `i8,i16,i32,u8,u16` |
| `f32`, `f64` | `f16`→`f32`→`f64` | float widening only |
| `String` | `ascii`,`utf8`,`utf16-le`,`utf32-le`,`latin1` | every text encoding → one `String` |
| `Vec<u8>` | **everything** | the universal sink; each input's raw payload |
| `bool` | `bool` | |
| `half::f16` | `f16` | only with the `half` feature |

**Integers and floats never mix**, even where a value would be exact. Call
`.promote(false)` to restrict a target to its exact native `decode-as`.

```rust
# use trickydata::Corpus;
# let c = Corpus::bundled();
let exact_u8: Vec<u8> = c.get::<u8>().promote(false).values();       // only decode-as u8
let i32s: Vec<i32> = c.get::<i32>().values();                        // i8,i16,i32,u8,u16
```

## Return modes

Three terminals pick the element type:

* `.values() -> Vec<T>` — the natural target values; deliberately-invalid inputs
  are excluded (they have no value), so the list is homogeneous.
* `.examples() -> Vec<Example<T>>` — rich objects (`.value()`, `.as_text()`,
  `.as_bytes()`, `.is_valid()`, `.unicode_meta()`, `.pairs()`, `.as_file()`).
* `.raw() -> Vec<Vec<u8>>` — each input's raw payload bytes.

## Invalid inputs

The corpus marks deliberately-invalid inputs with `invalid-as` (e.g. an
almost-valid UTF-8 sequence). They join the selection of the type they *would*
be, but only surface in `examples()`/`raw()` (toggle with `.include_invalid(..)`,
default on). `Example::value()` on such an input **returns `Err`** — it attempts
the real decode rather than silently handing back bytes. The raw payload is
always reachable via `.as_bytes()`.

```rust
# use trickydata::{Corpus, DecodeAs};
# let c = Corpus::bundled();
let bad = c.get::<String>().tags(["invalid-utf8"]).examples();
assert!(bad.iter().all(|e| e.value().is_err() && !e.as_bytes().is_empty()));
```

## Static data macros

For test suites that want compile-time data instead of constructing a `Corpus`,
`static_values!` and `static_examples!` expand selected corpus inputs into Rust
literals:

```rust
static SQL: &[&str] = trickydata::static_values!(String, tags = ["sql"]);
static FLOATS: &[f64] = trickydata::static_values!(f64);

static BAD_UTF8: &[trickydata::StaticExample<&str>] =
    trickydata::static_examples!(String, tags = ["invalid-utf8"]);
```

The macros support `tags`, `exclude_tags`, `names`, `promote`, and
`include_invalid`. `String` values are emitted as `&'static str`; `Vec<u8>` values
are emitted as `&'static [u8]`. Invalid would-be examples have `value: None` and
keep their raw bytes.

## Pairs

`pairs::<T>()` returns explicitly-linked pairs (filter with
`.hint(PairHint::Equal | NotEqual | Tricky)`) and, unless filtered, some
randomly-drawn *unrelated* pairs to stress comparison/dedup logic. `.max(n)` caps
the total; `.seed(s)` makes the random draw reproducible.

```rust
# use trickydata::{Corpus, PairHint};
# let c = Corpus::bundled();
let tricky = c.pairs::<f64>().hint(PairHint::Tricky).values();   // Vec<(f64, f64)>
```

## String sizing (feature `sizing`)

Pad tricky strings to an exact count of a Unicode unit (`ByteLength`,
`CodeUnits`, `CodePoints`, `ScalarValues`, `ExtendedGraphemes`). Each candidate is
left-padded and then **re-measured** to verify the exact count.

```rust
# use trickydata::{Corpus, SizeUnit};
# let c = Corpus::bundled();
let sized = c.get::<String>().size(SizeUnit::CodePoints, 8).examples();
```

`ExtendedGraphemes` (UAX #29) needs the `sizing` feature
(`unicode-segmentation`); the other units are dependency-free.

## Choosing a corpus

```rust
# use trickydata::Corpus;
let _ = Corpus::bundled();                              // embedded at compile time (latest)
let _ = Corpus::from_path("…/trickydata.trickydata");  // a local artifact
let _ = Corpus::from_version("0.1.0");                 // version seam (remote fetch is future)
```

## Features

| feature | effect |
|---------|--------|
| `sizing` | extended-grapheme-cluster sizing via `unicode-segmentation` |
| `half` | adds a native `half::f16` target type (f16 already promotes into `f32`/`f64`) |

## Development

```bash
cargo test --all-features
scripts/sync-data.sh     # refresh the vendored corpus from the Rust compiler output
```

The bundled `.trickydata` artifact lives in `data/` and is embedded with
`include_bytes!`.
