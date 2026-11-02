//! Grounding sources and their delivery contract: the flex token allocator and RAG source trait,
//! the cached source with its dirty handle, and the invalidation that marks grounding dirty.
//! Cut out of continuum-cognition as a leaf of its crate tree (docs/architecture/
//! COGNITION-CRATE-TREE.md, step 6); cognition re-exports each module under its old path.

pub mod rag_budget;
pub mod cached_source;
pub mod grounding_invalidation;
