//! Reconstruct-by-replay: a persona's causal acts are an executable log.
//!
//! Joel (2026-08-25), watching the operator recover a clobbered edit by re-running
//! the exact change from memory: "our causal engrams would also allow for a persona
//! an ability to rewind or reapply like you did here." Exactly. Every act-engram
//! already links by `CausedBy`/`Produced` (CAUSAL-MEMORY-GRAPH.md); what was missing
//! is that the act's memory be *executable*, not just prose — so a lost artifact (a
//! clobbered file, a crash mid-task, a bad merge) can be REAPPLIED from the record
//! of having done it, not re-derived.
//!
//! This module is the replayability layer: it captures the structured `ToolCall`
//! that produced an act (the executable half the prose engram drops), and — critically
//! — classifies whether replaying it is SAFE. That classification is the whole safety
//! story: replay is powerful precisely because it re-executes, so it must never
//! re-fire an irreversible or externally-visible act (a deploy, a message send, a
//! delete) on a "recover my work" impulse.
//!
//! Extends [[continuity-is-the-default-reset-is-the-exception]] from *resume by
//! recall* to *reconstruct by replay*. REWIND (undo to a decision point) is the
//! sibling: for code work her workspace is git, so `git reset` through her own hands
//! already provides it; general-state rewind needs inverse-acts/snapshots and is the
//! later, harder half. This module builds the high-leverage REAPPLY direction.

use crate::ai::types::ToolCall;

/// How safe an act is to re-execute from memory. The gate on reconstruct-by-replay:
/// only [`Replayable::Idempotent`] acts may be auto-reapplied; the rest surface to
/// the persona (or a human) as "this is what I did, but replaying it has
/// consequences — confirm."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replayable {
    /// Re-running produces the same result with no new side effect beyond restoring
    /// the intended state: a file write/edit (same bytes → same file), a workspace
    /// creation, a read/search (pure). SAFE to reapply to recover lost state.
    Idempotent,
    /// Re-running repeats an EXTERNAL, one-way, or accumulating effect: a message
    /// send, a git commit/push, a deploy, a delete, a background dispatch (spawns a
    /// second job). NEVER auto-replayed — replaying "I sent the release note" sends
    /// it twice. Surfaced for explicit confirmation only.
    HasSideEffects,
    /// Unknown verb — treated as [`Self::HasSideEffects`] by every gate (fail safe:
    /// an unclassified act is assumed consequential, never blindly re-fired).
    Unknown,
}

impl Replayable {
    /// May this act be AUTO-reapplied to reconstruct lost state? Only the idempotent
    /// class — everything else needs a human/persona decision.
    pub fn auto_replayable(self) -> bool {
        matches!(self, Replayable::Idempotent)
    }
}

/// The executable record of one act — the structured [`ToolCall`] (which the prose
/// engram drops) plus its replay classification. Captured at the act seam; a
/// persona's episodic memory of *doing* becomes re-runnable through this.
#[derive(Debug, Clone)]
pub struct ReplayableAct {
    /// The exact call that produced the act — name + args, re-executable verbatim
    /// through the persona's own hands (`command_executor`).
    pub call: ToolCall,
    /// Whether re-executing is safe (see [`Replayable`]).
    pub safety: Replayable,
}

impl ReplayableAct {
    /// Capture an act for replay, classifying its safety from the verb. The verb is
    /// the slash form or the model's underscore form — normalized before matching, so
    /// `code_write` and `code/write` classify identically.
    pub fn capture(call: ToolCall) -> Self {
        let safety = classify(&call.name);
        Self { call, safety }
    }
}

/// Classify a tool verb's replay safety. IDEMPOTENT: writes/edits (same content →
/// same file), workspace creation, and pure reads/searches — re-running restores
/// state without a new side effect. SIDE-EFFECTS: sends, commits/pushes, deploys,
/// deletes, background dispatch, shell (arbitrary — a shell command may `rm` or
/// `curl`, so it is never assumed pure). Unknown verbs fail safe to side-effects.
pub fn classify(verb: &str) -> Replayable {
    // Normalize the model's underscore form to the canonical slash form, same as the
    // act loop (`apply.rs`), so classification is dialect-independent.
    let v = verb.replace('_', "/");
    // Pure reads / analysis — replaying only re-observes, changing nothing.
    const PURE: &[&str] = &[
        "code/read", "code/tree", "code/list", "code/search", "code/grep",
        "perception/observe", "perception/look", "commands/list", "commands/help",
    ];
    // State-restoring writes — same content reproduces the intended artifact.
    const IDEMPOTENT_WRITES: &[&str] = &[
        "code/write", "code/edit", "code/create-workspace",
    ];
    // Explicitly consequential — one-way, external, or accumulating.
    const SIDE_EFFECTS: &[&str] = &[
        "chat/send", "code/shell", "code/run", "code/git/commit", "code/git/apply",
        "code/git/push", "models/pull", "forge/train", "benchmark/round",
        "cognition/full-evaluate", "code/delete", "code/remove",
    ];
    if PURE.contains(&v.as_str()) || IDEMPOTENT_WRITES.contains(&v.as_str()) {
        Replayable::Idempotent
    } else if SIDE_EFFECTS.contains(&v.as_str()) {
        Replayable::HasSideEffects
    } else {
        Replayable::Unknown // fail safe: unclassified verbs are treated as consequential
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str) -> ToolCall {
        ToolCall { id: "1".into(), name: name.into(), input: serde_json::json!({}) }
    }

    // what this catches: the safety gate that makes reconstruct-by-replay usable —
    // a write can be auto-reapplied to recover lost work (what the operator did by
    // hand here), but a send/deploy/commit/shell/delete NEVER auto-fires (replaying
    // it repeats the external effect), and an unknown verb fails SAFE to
    // side-effects. If this classification is wrong, "recover my work" could send a
    // message twice or re-run a deploy.
    #[test]
    fn only_idempotent_acts_auto_replay_everything_else_fails_safe() {
        // Writes + pure reads: safe to reapply, recovering state.
        for v in ["code/write", "code_edit", "code/read", "code/tree", "perception/look"] {
            let a = ReplayableAct::capture(call(v));
            assert!(a.safety.auto_replayable(), "{v} should auto-replay");
        }
        // Consequential acts: never auto-replayed.
        for v in ["chat/send", "code/shell", "code/git/commit", "models/pull", "code/delete"] {
            let a = ReplayableAct::capture(call(v));
            assert!(!a.safety.auto_replayable(), "{v} must NOT auto-replay");
            assert_eq!(a.safety, Replayable::HasSideEffects);
        }
        // Unknown verb → fail safe (assumed consequential).
        assert_eq!(classify("some/novel/verb"), Replayable::Unknown);
        assert!(!Replayable::Unknown.auto_replayable());
        // Dialect independence: underscore and slash classify identically.
        assert_eq!(classify("code_write"), classify("code/write"));
    }
}
