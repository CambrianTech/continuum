//! The capture of a mind's turns: the one fixture-directory policy and the prompt-capture
//! recorder, its per-path owner and its playback store. Cut out of continuum-cognition as a leaf
//! of its crate tree (docs/architecture/COGNITION-CRATE-TREE.md, step 9); cognition re-exports
//! each module under its old path.

/// Where captures land: the one fixture-directory policy shared by every live capture
/// (persona turns, prompt and workspace captures) and their readers.
pub mod capture;
pub mod prompt_capture;
