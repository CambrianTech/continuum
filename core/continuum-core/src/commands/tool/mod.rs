//! `tool/*` — the persona's affordances over her OWN tool usage.
//!
//! - `tool/conformance`: audit whether every tool she can reach is actually
//!   usable — the on-demand face of the #163 AI-usability harness.
//!
//! `tool/output` (recover and grep a flood-sized result the executor spilled) lives beside the
//! spill it reads, in `continuum_cognition::cognition::tool_executor::tool_output`, so every handle cognition
//! renders names it through its typed `TOOL_OUTPUT`.
//!
//! Stateless and AiSafe; a personal, not shared, surface.

pub mod conformance;
pub mod usage;
