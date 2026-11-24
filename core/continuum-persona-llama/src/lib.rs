//! The persona engine's [`Tier`](continuum_persona_engine::residency::Tier) over the fork's native
//! persona blocks: one libllama context, two execution streams, and every other mind's blocks held in
//! RAM as she left them. The pipeline ([`continuum_persona_engine::pipeline::Roles`]) decides when a
//! mind moves; this adapter only performs the move (a native save out, a native bind in) and reports
//! it landed.

mod unified_tier;

pub use unified_tier::{base_genes, Blocks, Decoded, Genes, Landed, LearnReply, Rebind, Step, Stream, UnifiedTier, STREAMS};
