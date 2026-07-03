//! The [`Query`] builder: select inputs from a corpus through a target type.
//!
//! The target type `T` (the generic on [`Corpus::get`]) determines which
//! inputs are selected; filters narrow the selection further; the three
//! terminal methods ([`Query::values`], [`Query::examples`], [`Query::raw`])
//! choose the element type.

use std::marker::PhantomData;

use crate::corpus::{Corpus, Input};
use crate::example::Example;
use crate::sizing::{self, SizeUnit};
use crate::target::Target;

/// How to order results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    /// Sort by input name (the default; deterministic).
    Name,
    /// Preserve corpus order (which is itself name-sorted at compile time).
    None,
}

/// A pending selection over a [`Corpus`], producing values of type `T`.
///
/// Created by [`Corpus::get`]. Chain filters, then call a terminal.
pub struct Query<'c, T> {
    corpus: &'c Corpus,
    tags: Vec<String>,
    exclude_tags: Vec<String>,
    names: Option<Vec<String>>,
    promote: bool,
    include_invalid: bool,
    max: Option<usize>,
    sort: Sort,
    size: Option<(SizeUnit, usize)>,
    pad: char,
    _t: PhantomData<fn() -> T>,
}

impl<'c, T: Target> Query<'c, T> {
    pub(crate) fn new(corpus: &'c Corpus) -> Self {
        Query {
            corpus,
            tags: Vec::new(),
            exclude_tags: Vec::new(),
            names: None,
            promote: true,
            include_invalid: true,
            max: None,
            sort: Sort::Name,
            size: None,
            pad: 'a',
            _t: PhantomData,
        }
    }

    /// Require all of these tags (logical AND).
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

    /// Restrict to an explicit set of input names.
    pub fn names<I, S>(mut self, names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.names = Some(names.into_iter().map(Into::into).collect());
        self
    }

    /// Enable/disable lossless promotion. Default `true`.
    pub fn promote(mut self, yes: bool) -> Self {
        self.promote = yes;
        self
    }

    /// Include deliberately-invalid inputs in the `examples`/`raw` views. Default
    /// `true`. Has no effect on [`Query::values`] (invalid inputs have no value).
    pub fn include_invalid(mut self, yes: bool) -> Self {
        self.include_invalid = yes;
        self
    }

    /// Cap the number of results.
    pub fn max(mut self, n: usize) -> Self {
        self.max = Some(n);
        self
    }

    /// Set the result ordering. Default [`Sort::Name`].
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort = sort;
        self
    }

    /// Left-pad text to exactly `target` of a Unicode `unit` (strings only).
    ///
    /// # Panics
    /// If `T` is not [`String`]. Grapheme sizing additionally requires the
    /// `sizing` crate feature.
    pub fn size(mut self, unit: SizeUnit, target: usize) -> Self {
        assert!(
            T::SUPPORTS_SIZING,
            "sizing is only supported for the String target"
        );
        self.size = Some((unit, target));
        self
    }

    /// Set the padding character used by [`Query::size`]. Default `'a'`.
    pub fn pad(mut self, c: char) -> Self {
        self.pad = c;
        self
    }

    // --- terminals -------------------------------------------------------

    /// The natural target values. Deliberately-invalid inputs are excluded (they
    /// have no value), so the result is homogeneous `Vec<T>`.
    pub fn values(self) -> Vec<T> {
        self.build(false)
            .iter()
            .filter_map(|e| e.value().ok())
            .collect()
    }

    /// Rich [`Example`] objects. Includes invalid inputs when `include_invalid`.
    pub fn examples(self) -> Vec<Example<T>> {
        self.build(true)
    }

    /// Each input's raw payload bytes. Includes invalid inputs when
    /// `include_invalid`.
    pub fn raw(self) -> Vec<Vec<u8>> {
        self.build(true)
            .iter()
            .map(|e| e.as_bytes().to_vec())
            .collect()
    }

    // --- internals -------------------------------------------------------

    /// Select, build examples, size, sort, and cap. `invalid_mode` is true for
    /// the example/raw terminals; invalid inputs are only ever surfaced there
    /// (and only when `include_invalid` and not sizing).
    fn build(&self, invalid_mode: bool) -> Vec<Example<T>> {
        let select_invalid = invalid_mode && self.include_invalid && self.size.is_none();
        let selected = self.select(select_invalid);
        let mut examples: Vec<Example<T>> =
            selected.iter().map(|i| Example::from_input(i)).collect();

        if let Some((unit, target)) = self.size {
            examples = sizing::size_examples(examples, unit, target, self.pad);
        }

        if self.sort == Sort::Name {
            examples.sort_by(|a, b| a.name().cmp(b.name()));
        }
        if let Some(max) = self.max {
            examples.truncate(max);
        }
        examples
    }

    /// Membership by target + tag filters, with deliberately-invalid inputs
    /// joining their would-be type's selection.
    fn select(&self, include_invalid: bool) -> Vec<&'c Input> {
        let mut out = Vec::new();
        for input in &self.corpus.inputs {
            if !self.is_member(input, include_invalid) {
                continue;
            }
            if let Some(names) = &self.names {
                if !names.iter().any(|n| n == &input.name) {
                    continue;
                }
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

    fn is_member(&self, input: &Input, include_invalid: bool) -> bool {
        if T::accepts(input.decode_as, self.promote) {
            return true;
        }
        // A deliberately-invalid input joins the selection of the type it *would*
        // be (tested without promotion), when invalid inputs are wanted.
        if let Some(invalid_as) = input.invalid_as {
            if include_invalid && T::accepts(invalid_as, false) {
                return true;
            }
        }
        false
    }
}
