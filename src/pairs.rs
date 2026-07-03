//! [`PairQuery`]: interesting *pairs* of inputs from a target type.
//!
//! Explicit corpus links come first (filterable by [`PairHint`]); unless a
//! filter constrains the result, randomly drawn *unrelated* pairs are appended
//! to stress comparison/dedup logic. A seed makes the random draw reproducible
//! (via the in-crate [`crate::rng`]).

use std::collections::HashSet;
use std::marker::PhantomData;

use crate::corpus::{Corpus, Input, PairHint};
use crate::example::Example;
use crate::rng::SplitMix64;
use crate::target::Target;

const UNRELATED_ATTEMPT_LIMIT: usize = 10_000;
const DEFAULT_MAX: usize = 50;

/// Two inputs presented together. `related` is true for an explicit corpus link
/// (with a `hint`), false for a randomly drawn unrelated pair.
pub struct Pair<T> {
    pub a: Example<T>,
    pub b: Example<T>,
    /// The relationship for an explicit link; `None` for an unrelated pair.
    pub hint: Option<PairHint>,
    /// Whether this is an explicit corpus link (vs a random unrelated draw).
    pub related: bool,
}

impl<T> IntoIterator for Pair<T> {
    type Item = Example<T>;
    type IntoIter = std::array::IntoIter<Example<T>, 2>;

    fn into_iter(self) -> Self::IntoIter {
        [self.a, self.b].into_iter()
    }
}

/// A pending pair selection over a [`Corpus`]. Created by [`Corpus::pairs`].
pub struct PairQuery<'c, T> {
    corpus: &'c Corpus,
    hint: Option<PairHint>,
    tags: Vec<String>,
    exclude_tags: Vec<String>,
    promote: bool,
    include_invalid: bool,
    include_unrelated: Option<bool>,
    max: Option<usize>,
    seed: Option<u64>,
    _t: PhantomData<fn() -> T>,
}

impl<'c, T: Target> PairQuery<'c, T> {
    pub(crate) fn new(corpus: &'c Corpus) -> Self {
        PairQuery {
            corpus,
            hint: None,
            tags: Vec::new(),
            exclude_tags: Vec::new(),
            promote: true,
            include_invalid: true,
            include_unrelated: None,
            max: Some(DEFAULT_MAX),
            seed: None,
            _t: PhantomData,
        }
    }

    /// Keep only explicit links with this relationship.
    pub fn hint(mut self, hint: PairHint) -> Self {
        self.hint = Some(hint);
        self
    }

    /// Require all of these tags on both inputs (via selection).
    pub fn tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Exclude any input carrying any of these tags.
    pub fn exclude_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.exclude_tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// Enable/disable lossless promotion in selection. Default `true`.
    pub fn promote(mut self, yes: bool) -> Self {
        self.promote = yes;
        self
    }

    /// Include deliberately-invalid inputs in the `examples`/`raw` views.
    pub fn include_invalid(mut self, yes: bool) -> Self {
        self.include_invalid = yes;
        self
    }

    /// Force the random *unrelated* pairs on or off. Defaults to on unless a
    /// `hint`/tag filter is set.
    pub fn include_unrelated(mut self, yes: bool) -> Self {
        self.include_unrelated = Some(yes);
        self
    }

    /// Cap the total number of pairs. Default `50`. Pass `usize::MAX` for no cap.
    pub fn max(mut self, n: usize) -> Self {
        self.max = Some(n);
        self
    }

    /// Seed the unrelated-pair draw for reproducibility.
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    // --- terminals -------------------------------------------------------

    /// Pairs of natural target values (invalid inputs excluded).
    pub fn values(self) -> Vec<(T, T)> {
        self.build(false)
            .into_iter()
            .filter_map(|p| match (p.a.value(), p.b.value()) {
                (Ok(a), Ok(b)) => Some((a, b)),
                _ => None,
            })
            .collect()
    }

    /// Rich [`Pair`] objects.
    pub fn examples(self) -> Vec<Pair<T>> {
        self.build(true)
    }

    /// Pairs of raw payload bytes.
    pub fn raw(self) -> Vec<(Vec<u8>, Vec<u8>)> {
        self.build(true)
            .into_iter()
            .map(|p| (p.a.as_bytes().to_vec(), p.b.as_bytes().to_vec()))
            .collect()
    }

    // --- internals -------------------------------------------------------

    fn build(&self, invalid_mode: bool) -> Vec<Pair<T>> {
        let select_invalid = invalid_mode && self.include_invalid;
        let selected = self.select(select_invalid);
        let names: HashSet<&str> = selected.iter().map(|i| i.name.as_str()).collect();

        // Explicit links: dedup by unordered name pair, track every linked pair
        // (for excluding from the unrelated draw).
        let mut explicit: Vec<Pair<T>> = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut all_linked: HashSet<(String, String)> = HashSet::new();

        for input in &selected {
            for link in &input.links {
                if !names.contains(link.name.as_str()) {
                    continue;
                }
                all_linked.insert(key(&input.name, &link.name));
                if let Some(want) = self.hint {
                    if link.hint != want {
                        continue;
                    }
                }
                let k = key(&input.name, &link.name);
                if !seen.insert(k) {
                    continue;
                }
                let a = self.example(&selected, &input.name);
                let b = self.example(&selected, &link.name);
                explicit.push(Pair {
                    a,
                    b,
                    hint: Some(link.hint),
                    related: true,
                });
            }
        }

        explicit.sort_by(|p, q| (p.a.name(), p.b.name()).cmp(&(q.a.name(), q.b.name())));

        let filtered =
            self.hint.is_some() || !self.tags.is_empty() || !self.exclude_tags.is_empty();
        let include_unrelated = self.include_unrelated.unwrap_or(!filtered);

        let mut result = explicit;
        if let Some(max) = self.max {
            result.truncate(max);
        }

        if include_unrelated {
            let want = self.max.map(|m| m.saturating_sub(result.len()));
            result.extend(self.unrelated(&selected, &all_linked, &seen, want));
        }
        result
    }

    fn example(&self, selected: &[&'c Input], name: &str) -> Example<T> {
        let input = selected
            .iter()
            .find(|i| i.name == name)
            .expect("name came from the selected set");
        Example::from_input(input)
    }

    /// Randomly draw unrelated pairs: distinct names that are neither explicitly
    /// linked nor already used. Deterministic given `seed`.
    fn unrelated(
        &self,
        selected: &[&'c Input],
        all_linked: &HashSet<(String, String)>,
        used: &HashSet<(String, String)>,
        want: Option<usize>,
    ) -> Vec<Pair<T>> {
        let mut names: Vec<&str> = selected.iter().map(|i| i.name.as_str()).collect();
        names.sort_unstable();
        if names.len() < 2 || matches!(want, Some(0)) {
            return Vec::new();
        }

        let mut rng = SplitMix64::new(self.seed.unwrap_or(0));
        let mut seen = used.clone();
        let mut out: Vec<Pair<T>> = Vec::new();
        let mut attempts = 0;

        while attempts < UNRELATED_ATTEMPT_LIMIT && want.map_or(true, |w| out.len() < w) {
            attempts += 1;
            let i = rng.below(names.len());
            let j = rng.below(names.len());
            if i == j {
                continue;
            }
            let (a, b) = (names[i], names[j]);
            let k = key(a, b);
            if seen.contains(&k) || all_linked.contains(&k) {
                continue;
            }
            seen.insert(k);
            out.push(Pair {
                a: self.example(selected, a),
                b: self.example(selected, b),
                hint: None,
                related: false,
            });
            // Sane cap when uncapped: don't exceed the number of names.
            if want.is_none() && out.len() >= names.len() {
                break;
            }
        }
        out
    }

    /// Shares its shape with `Query::select`.
    fn select(&self, include_invalid: bool) -> Vec<&'c Input> {
        let mut out = Vec::new();
        for input in &self.corpus.inputs {
            let member = T::accepts(input.decode_as, self.promote)
                || input
                    .invalid_as
                    .is_some_and(|ia| include_invalid && T::accepts(ia, false));
            if !member {
                continue;
            }
            if !self.tags.iter().all(|t| input.tags.contains(t)) {
                continue;
            }
            if self.exclude_tags.iter().any(|t| input.tags.contains(t)) {
                continue;
            }
            out.push(input);
        }
        out
    }
}

/// Unordered name-pair key for dedup.
fn key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}
