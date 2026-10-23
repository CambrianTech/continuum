//! The `[mission]` source id. The retired `agent/solve` pinned the task brief as a
//! `[mission]` grounding block (the #390 anchor-loss fix); the id stays because
//! `deliberation_prompt` still classifies that block as standing framing.

/// Source identifier — renders as the `[mission]` grounding block via the
/// generic `[<source_id>]` projection.
pub(crate) const SOURCE_ID: &str = "mission";
