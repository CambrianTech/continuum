//! One home for compiling a **source-literal** regex pattern.
//!
//! `Regex::new` can only fail on a malformed pattern. When the pattern is a literal in
//! this crate's source, that failure is a programmer error fixed by editing the literal,
//! never a runtime condition a caller could handle — so it is not modelled as a `Result`
//! threaded through every text check. The `&'static str` parameter is the boundary: a
//! runtime-built (user- or model-supplied) pattern cannot reach this function and must
//! use `Regex::new` and handle its error.
//!
//! Every `LazyLock<Regex>` built through here is forced by its owning module's tests
//! (`garbage_detection`, `response_cleaning`, `mention_detection`, `persona::response`),
//! so a malformed literal fails `cargo test`, not a live turn.

use regex::Regex;

/// Compile a regex pattern that is a literal in this crate's source.
pub(crate) fn literal(pattern: &'static str) -> Regex {
    Regex::new(pattern).expect("source-literal regex pattern is malformed") // literal-only (&'static str): a bad pattern is a source typo the owning module's tests catch on first force
}
