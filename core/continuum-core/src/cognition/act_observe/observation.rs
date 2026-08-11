//! The TYPED observation: one executed act as a first-class value threaded
//! end-to-end through the act→observe→re-perceive contract, replacing the
//! `format!`-flattened `String` the whole downstream used to re-parse.
//!
//! See docs/cognition/TYPED-OBSERVATION-REFACTOR.md. The bug this kills
//! (run-18057-f1): a persona's tool RESULT did not reliably re-enter her next
//! LLM prompt — it flowed as untyped prose routed as a perception *bid* an
//! arbiter could silently evict, and a dozen predicates re-parsed that rendered
//! prose. Here the typed pair `(ToolCall, ToolResult)` — correlated by
//! `tool_use_id == call.id`, precomputed `verb`/`paths` — is the single source,
//! and the two renderings (recency / recall) become PURE FUNCTIONS of it so no
//! consumer re-derives structure from the receipt string.
//!
//! All types derive `Serialize, Deserialize` (they land in `VolatileSnapshot` →
//! `~/.continuum/personas/<id>/volatile.json`, also the grid-sync wire format)
//! and `#[ts(export)]` (`ToolCall`/`ToolResult` already cross the Rust→TS
//! boundary, so a struct built from them must too — ts-rs law, CLAUDE.md).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ai::types::{ToolCall, ToolResult};
use crate::cognition::context_budget::ContextBudget;

use super::recency::{
    bound_recency_result, humanize_result_content, render_act_for_recall,
    summarize_args_for_recency,
};

/// Semantic class of a tool verb. Produced ONCE from `ToolCall.name` at the act
/// seam; every predicate that used to grep the rendered receipt for a verb
/// prefix now reads this. Single home of the verb→class mapping duplicated as
/// (a) the `wrote` bool in `apply.rs`, (b) the "I ran code/write(" scans in
/// `perception.rs`, (c) the orientation-prefix scans in `is_redundant_orientation`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ToolVerb.ts")]
pub enum ToolVerb {
    Write,
    Edit,
    Apply,
    Commit, // mutate disk
    Run,
    Shell,
    Read,
    Screenshot, // observe the world
    ListCommands,
    Help,
    Tree,
    Search, // orient
    Other,
}

impl ToolVerb {
    /// Normalizes `_`→`/` first (models emit both forms — same as the `wrote`
    /// computation in `apply.rs`).
    pub fn classify(name: &str) -> Self {
        let n = name.replace('_', "/");
        // exact SUBSTRING semantics preserved from the `wrote` bool so it is unchanged.
        if n.contains("write") {
            return ToolVerb::Write;
        }
        if n.contains("edit") {
            return ToolVerb::Edit;
        }
        if n.contains("apply") {
            return ToolVerb::Apply;
        }
        if n.contains("commit") {
            return ToolVerb::Commit;
        }
        match n.as_str() {
            "commands/list" => ToolVerb::ListCommands,
            "commands/help" => ToolVerb::Help,
            "code/tree" => ToolVerb::Tree,
            "code/search" => ToolVerb::Search,
            "code/run" => ToolVerb::Run,
            "code/shell" => ToolVerb::Shell,
            "code/read" => ToolVerb::Read,
            "interface/screenshot" => ToolVerb::Screenshot,
            _ => ToolVerb::Other,
        }
    }

    /// Reached DISK — replaces the `mutated_workspace` / `wrote` bool scan.
    pub fn mutates(&self) -> bool {
        matches!(
            self,
            ToolVerb::Write | ToolVerb::Edit | ToolVerb::Apply | ToolVerb::Commit
        )
    }

    /// Looked at the world — replaces the observation-verb class scan in
    /// `wrote_without_observation`.
    pub fn observes(&self) -> bool {
        matches!(
            self,
            ToolVerb::Run | ToolVerb::Shell | ToolVerb::Read | ToolVerb::Screenshot
        )
    }

    /// Surveys what the mind already carries — replaces the `is_redundant_orientation`
    /// prefix scan. `code/search` is NOT orientation (it reads specific content), so it
    /// is excluded here exactly as `is_orientation_call` excludes it.
    pub fn is_orientation(&self) -> bool {
        matches!(
            self,
            ToolVerb::ListCommands | ToolVerb::Help | ToolVerb::Tree
        )
    }
}

/// The typed payload of ONE tool call's result. REPLACES the `format!` blob.
/// `result` reuses the existing `ai::types::ToolResult` verbatim
/// (`tool_use_id == ToolCall.id`). `verb`/`paths` PRECOMPUTED at the act seam so
/// no consumer re-derives from prose.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ToolOutput.ts")]
pub struct ToolOutput {
    /// Single source of the raw payload; correlated by `tool_use_id == call.id`.
    pub result: ToolResult,
    /// Computed once via `ToolVerb::classify(call.name)`.
    pub verb: ToolVerb,
    /// Files touched, from `call.input` — exact membership, immune to head-truncation.
    #[ts(type = "Array<string>")]
    pub paths: Vec<PathBuf>,
}

/// Per-call outcome. Flattens the FIVE return sites of the old `Option<String>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ActStatus.ts")]
pub enum ActStatus {
    Executed,
    /// Executor `Err` — the old path only `warn`'d and dropped this.
    Errored { message: String },
    /// The already-satisfied short-circuit.
    AlreadySatisfied { repeat: usize },
    /// The redundant-orientation short-circuit.
    RedundantOrientation { repeat: usize },
}

/// ONE act = typed pair (call, output) + status. `call` retains `ToolCall`
/// (INCLUDING `.id`) so correlation is by id, not by `outcome.results.get(i)`
/// positional index.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/Observation.ts")]
pub struct Observation {
    pub call: ToolCall,
    pub output: ToolOutput,
    pub status: ActStatus,
}

/// The BATCH result of `apply_act` — replaces `Option<String>`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ActOutcome.ts")]
pub enum ActOutcome {
    /// The mind has no hands (tools were never offered) — was `None`.
    NoHands,
    /// The executor batch failed at the channel level — was `None`, now
    /// distinguishable from no-hands for a future backstop.
    ExecutorError {
        calls: Vec<ToolCall>,
        message: String,
    },
    /// One or more acts ran (or were short-circuited) — was `Some(observation)`.
    Acted { acts: Vec<Observation> },
}

impl ActOutcome {
    /// The Some/None signal `settle_step` used becomes typed.
    pub fn produced_an_act(&self) -> bool {
        matches!(self, ActOutcome::Acted { acts } if !acts.is_empty())
    }
}

impl Observation {
    /// Humanized result body — the ONE decode both renderings share (card
    /// 0a4c0648: the executor hands back serde-serialized JSON; render code as
    /// code, never escape-soup).
    fn body_text(&self) -> String {
        humanize_result_content(&self.output.result.content)
    }

    fn is_err(&self) -> bool {
        self.output.result.is_error == Some(true)
    }

    /// The RECENCY-channel rendering: the FULL trace working memory keeps so the
    /// mind can act on what it just fetched. Byte-identical to the old inline
    /// `apply.rs` block (`{name}({args}){because}\nResult:\n{bounded}\n\n`) — the
    /// #205 KV-stability invariant. `intent` is batch-level (one per `apply_act`
    /// call, shared by every act in the batch), so it is passed rather than
    /// stored per-`Observation`.
    pub fn render_recency(&self, intent: &str, budget: &ContextBudget) -> String {
        let fold = Some(budget.echoed_arg_chars());
        let args = summarize_args_for_recency(&self.call.input, fold);
        let because = if intent.trim().is_empty() {
            String::new()
        } else {
            format!(" because {}", intent.trim())
        };
        format!(
            "{}({}){}\nResult:\n{}\n\n",
            self.call.name,
            args,
            because,
            bound_recency_result(&self.body_text(), budget),
        )
    }

    /// The RECALL-channel rendering: the COLLAPSED reference the Episodic engram
    /// re-injects on later turns (#166 — a separate channel from recency). The
    /// body is `render_act_for_recall`, unchanged.
    pub fn render_recall(&self, intent: &str) -> String {
        render_act_for_recall(
            &self.call.name,
            &self.call.input,
            intent,
            self.is_err(),
            &self.body_text(),
        )
    }
}

/// Extract the file paths a tool call NAMES: any string under a `file_path`,
/// `path`, or `paths` key in the call's input. Exact typed membership from HER
/// OWN call — immune to receipt head-truncation, never inferred. Mirrors the
/// key set `collect_touched_paths` scans, plus a `paths` array.
pub fn extract_paths(input: &serde_json::Value) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut push = |p: &str| {
        let pb = PathBuf::from(p);
        if !out.contains(&pb) {
            out.push(pb);
        }
    };
    for key in ["file_path", "path"] {
        if let Some(p) = input.get(key).and_then(|v| v.as_str()) {
            push(p);
        }
    }
    if let Some(arr) = input.get("paths").and_then(|v| v.as_array()) {
        for p in arr.iter().filter_map(|v| v.as_str()) {
            push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the verb→class mapping is the ONE home the `wrote` bool,
    // the "I ran code/write(" scans, and the orientation-prefix scans all collapse
    // into (run-18057-f1 / seam-5). `mutates()` must agree with the old `wrote`
    // substring rule (write/edit/apply/commit, underscore-normalized), `observes()`
    // with the run/read/screenshot class, `is_orientation()` with list/help/tree
    // (never code/search — that reads content).
    #[test]
    fn tool_verb_classifies_the_load_bearing_classes() {
        assert!(ToolVerb::classify("code/write").mutates());
        assert!(ToolVerb::classify("code/edit").mutates());
        assert!(ToolVerb::classify("code/apply").mutates());
        assert!(ToolVerb::classify("code/commit").mutates());
        // underscore form (models emit both) normalizes the same way
        assert!(ToolVerb::classify("code_write").mutates());
        assert!(!ToolVerb::classify("code/read").mutates());

        assert!(ToolVerb::classify("code/run").observes());
        assert!(ToolVerb::classify("code/shell").observes());
        assert!(ToolVerb::classify("code/read").observes());
        assert!(ToolVerb::classify("interface/screenshot").observes());
        assert!(!ToolVerb::classify("code/write").observes());

        assert!(ToolVerb::classify("commands/list").is_orientation());
        assert!(ToolVerb::classify("commands/help").is_orientation());
        assert!(ToolVerb::classify("code/tree").is_orientation());
        // code/search reads specific content — NOT a survey, exactly as
        // is_orientation_call excludes it.
        assert!(!ToolVerb::classify("code/search").is_orientation());
        assert_eq!(ToolVerb::classify("code/search"), ToolVerb::Search);
        assert_eq!(ToolVerb::classify("work/list"), ToolVerb::Other);
    }

    // what this catches: exact typed path membership from the call's own input —
    // the field that replaces re-deriving a filename from head-truncated prose
    // (claimed_file_without_act / touched_paths). file_path, path, and a paths[]
    // array all land; duplicates dedupe; a call with none yields empty.
    #[test]
    fn extract_paths_reads_the_typed_input_not_the_receipt() {
        let one = serde_json::json!({ "file_path": "sympy/core/basic.py" });
        assert_eq!(extract_paths(&one), vec![PathBuf::from("sympy/core/basic.py")]);

        let arr = serde_json::json!({ "paths": ["a.rs", "b.rs", "a.rs"] });
        assert_eq!(
            extract_paths(&arr),
            vec![PathBuf::from("a.rs"), PathBuf::from("b.rs")],
            "paths array is read and deduped"
        );

        let none = serde_json::json!({ "query": "needle" });
        assert!(extract_paths(&none).is_empty());
    }
}
