//! `tool/*` — the persona's affordances over her OWN tool usage.
//!
//! - `tool/output`: recover and grep a flood-sized tool result the executor
//!   spilled to disk (tier 2 of the flood protection).
//! - `tool/conformance`: audit whether every tool she can reach is actually
//!   usable — the on-demand face of the #163 AI-usability harness.
//!
//! Stateless and AiSafe; a personal, not shared, surface.

pub mod conformance;
pub mod output;
