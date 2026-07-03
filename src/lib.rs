//! **trickydata** — a corpus of deliberately tricky test inputs (edge-case
//! Unicode, IEEE-754 floats, boundary integers, SQL, binary blobs), as a native
//! Rust client.
//!
//! This client is **generic over the target Rust type** and uses **lossless
//! promotion**: asking for a wider type pulls in any narrower input that
//! converts without loss.
//!
//! ```
//! use trickydata::{Corpus, PairHint, SizeUnit};
//!
//! let c = Corpus::bundled();
//!
//! // u8 + u16 inputs, every value losslessly a u16:
//! let v: Vec<u16> = c.get::<u16>().values();
//!
//! // only inputs whose decode-as is exactly u8:
//! let exact: Vec<u8> = c.get::<u8>().promote(false).values();
//!
//! // SQL edge cases (decode-as ascii/utf8) surface under String:
//! let sql: Vec<String> = c.get::<String>().tags(["sql"]).values();
//!
//! // the same inputs as raw bytes (Vec<u8> is the universal sink):
//! let sql_bytes: Vec<Vec<u8>> = c.get::<Vec<u8>>().tags(["sql"]).raw();
//!
//! // every IEEE-754 edge value as f64 (f16/f32/f64 promote in):
//! let floats: Vec<f64> = c.get::<f64>().values();
//!
//! // interesting float pairs:
//! let tricky = c.pairs::<f64>().hint(PairHint::Tricky).values();
//! # let _ = (exact, sql, sql_bytes, floats, tricky, SizeUnit::CodePoints);
//! ```
//!
//! For compile-time fixtures, [`static_values!`] and [`static_examples!`] expand
//! corpus selections into Rust literals:
//!
//! ```
//! static SQL: &[&str] = trickydata::static_values!(String, tags = ["sql"]);
//! static FLOATS: &[f64] = trickydata::static_values!(f64);
//! ```
//!
//! Point at a different corpus with [`Corpus::from_path`] or [`Corpus::from_version`].

mod corpus;
mod decode_as;
mod error;
mod example;
mod native;
mod pairs;
mod query;
mod resolve;
mod rng;
mod sizing;
mod static_data;
mod target;

pub use corpus::{Corpus, Input, PairHint, PairLink, UnicodeMeta};
pub use decode_as::DecodeAs;
pub use error::{DecodeError, NotTextError, ParseError, ResolveError};
pub use example::{Example, TempFile};
pub use native::Native;
pub use pairs::{Pair, PairQuery};
pub use query::{Query, Sort};
pub use sizing::SizeUnit;
pub use static_data::StaticExample;
pub use target::Target;
pub use trickydata_macros::{static_examples, static_values};

impl Corpus {
    /// Start a query for inputs viewable as the target type `T`.
    ///
    /// `T` may be any integer (`u8`..`u128`, `i8`..`i128`), `f32`/`f64`, `bool`,
    /// [`String`] (unifying every text encoding), or `Vec<u8>` (the universal
    /// sink). With the `half` feature, [`half::f16`] is also a target.
    pub fn get<T: Target>(&self) -> Query<'_, T> {
        Query::new(self)
    }

    /// Start a query for interesting *pairs* of inputs viewable as `T`.
    pub fn pairs<T: Target>(&self) -> PairQuery<'_, T> {
        PairQuery::new(self)
    }
}
