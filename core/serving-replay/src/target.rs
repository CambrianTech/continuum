//! The seam every engine under test sits behind. Today's llama-server implements it
//! (`llama_server`); S1's RAM page verbs and S5's `continuum-serving` port implement the
//! same four verbs, so one replay measures every slice against the same sequence.
//!
//! The seam is keyed by MIND, not by slot: which seat a mind occupies, and whom to evict
//! to seat her, belongs to the engine (S5's `seq_table` + `scheduler`). An adapter for an
//! engine that exposes raw slots (llama-server) carries today's core policy inside itself.

use serde::Serialize;

/// A mind in the replay: a persona id from a capture, or `mind-<n>` in a synthetic run.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
pub struct MindId(pub String);

impl std::fmt::Display for MindId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One chat message of an act, as the mind's request carried it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

/// What a mind submits for one act. The engine renders it with its own template, so
/// template fidelity stays the engine's (SERVING-ENGINE.md §6, "one renderer").
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActRequest {
    pub system: Option<String>,
    pub messages: Vec<Message>,
    /// OpenAI-shape tool definitions (`{"type":"function","function":{..}}`).
    pub tools: Vec<serde_json::Value>,
    /// Tokens to generate. The replay's next act comes from the capture, not from this
    /// output, so a short generation measures prefill and paging without decode cost.
    pub n_predict: u32,
}

/// How a mind's KV reached her seat for an act; the live core's `PageIn`
/// (continuum-engine `inference/turn_admission.rs`), named the same so receipts compare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PageInKind {
    /// Her KV was already in the seat.
    Resident,
    /// Her saved page was loaded into the seat.
    Restored,
    /// Nothing of hers was saved: her first act, or a page that was never written.
    Cold,
    /// A page was due and the engine refused it, so the act re-prefills in full.
    RestoreRejected,
}

impl PageInKind {
    /// The kind as the report and receipts spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Resident => "resident",
            Self::Restored => "restored",
            Self::Cold => "cold",
            Self::RestoreRejected => "restore_rejected",
        }
    }
}

/// What `page_out` moved and what it cost.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageOutReceipt {
    pub mind: MindId,
    pub tokens: u64,
    pub bytes: u64,
    pub ms: f64,
}

/// What `page_in` did to seat a mind, including the mind it displaced, if any.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageInReceipt {
    pub kind: PageInKind,
    pub tokens_restored: u64,
    pub ms: f64,
    pub evicted: Option<PageOutReceipt>,
}

/// The engine's own accounting of one act.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ActOutcome {
    /// Tokens of the rendered prompt.
    pub prompt_tokens: u64,
    /// Prompt tokens the engine reused from the seat's KV.
    pub cached_tokens: u64,
    /// Prompt tokens the engine prefilled for this act.
    pub fresh_tokens: u64,
    pub generated_tokens: u64,
    pub prompt_ms: f64,
    pub generate_ms: f64,
}

/// What the engine is and what happened to it over the run.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineStats {
    /// Short name of the implementation (`llama-server`, `fake`, later `continuum-serving`).
    pub engine: String,
    /// Minds the engine can hold resident at once.
    pub seats: u32,
    /// Times the engine process was replaced during the run. `None` when the target cannot
    /// observe it (an engine it attached to rather than launched).
    pub relaunches: Option<u32>,
    /// The exact command line, when the target launched the engine.
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetError(pub String);

impl std::fmt::Display for TargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TargetError {}

/// The four verbs an engine under test answers. The replay calls `page_in` before every
/// act (a resident mind answers `Resident` at no cost), then `act`.
pub trait ServingTarget {
    /// Seat `mind`, paging her in and paging out whomever the engine chooses to displace.
    fn page_in(&mut self, mind: &MindId) -> Result<PageInReceipt, TargetError>;
    /// Run one act for a seated mind.
    fn act(&mut self, mind: &MindId, request: &ActRequest) -> Result<ActOutcome, TargetError>;
    /// Move a seated mind's KV out of her seat (cooling her).
    fn page_out(&mut self, mind: &MindId) -> Result<PageOutReceipt, TargetError>;
    fn stats(&mut self) -> Result<EngineStats, TargetError>;
}
