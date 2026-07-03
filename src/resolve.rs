//! Resolve a [`Corpus`] from one of three sources:
//!
//! * **bundled** — the artifact embedded at compile time (`data/`);
//! * **a path** — a local `.trickydata` artifact;
//! * **a version** — a deliberate seam. There is no public host yet, so it
//!   serves the bundled artifact when the version matches and otherwise returns
//!   [`ResolveError::VersionUnavailable`].

use std::fs;
use std::path::Path;

use crate::corpus::Corpus;
use crate::error::ResolveError;

/// The corpus artifact, embedded at compile time.
const BUNDLED_ARTIFACT: &[u8] = include_bytes!("../data/trickydata.trickydata");

impl Corpus {
    /// Load the corpus bundled into this build (the latest as of compile time).
    ///
    /// # Panics
    /// Only if the embedded artifact is corrupt, which is a build-time invariant.
    pub fn bundled() -> Corpus {
        Corpus::from_trickydata_bytes(BUNDLED_ARTIFACT).expect("bundled corpus artifact is valid")
    }

    /// The version string of the bundled corpus, without fully loading it.
    pub fn bundled_version() -> String {
        Corpus::bundled().version().to_string()
    }

    /// Load a corpus from a local `.trickydata` file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Corpus, ResolveError> {
        let data = fs::read(path)?;
        Ok(Corpus::from_trickydata_bytes(&data)?)
    }

    /// Load a specific corpus version. Today this only serves the bundled
    /// artifact (for the matching version or `"latest"`); any other version is a
    /// not-yet-built remote-fetch seam.
    pub fn from_version(version: &str) -> Result<Corpus, ResolveError> {
        let bundled = Corpus::bundled();
        if version == bundled.version() || version == "latest" {
            return Ok(bundled);
        }
        Err(ResolveError::VersionUnavailable {
            requested: version.to_string(),
            bundled: bundled.version().to_string(),
        })
    }
}
