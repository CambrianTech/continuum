//! continuum-perception: a mind's senses (`perception/*` commands: observe, look, interact,
//! hot-edit) and the RAG engine that assembles her context, cut out of continuum-mind
//! (MODULE-ARCHITECTURE.md section 9.2, card 2f5b275b). Depends only on the live layer and
//! below; continuum-mind re-exports both modules under their old paths.

pub use continuum_live::*;

pub mod perception;
pub mod rag;
pub mod search;
