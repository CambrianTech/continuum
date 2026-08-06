//! Whether the work done under a command is admitted into the LIVING persona's memory.
//!
//! # Why this is a type and not a `bool`
//!
//! It was a `bool` — an `Option<bool>` named `learn`, resolved by `unwrap_or` in each module
//! that read it. On 2026-08-06 BigMama read both readers before wiring the consolidator and
//! found they disagreed:
//!
//! ```text
//! commands/agent/solve.rs   p.learn.unwrap_or(TRUE)    <- learns
//! cognition/eval.rs         p.learn.unwrap_or(FALSE)   <- safe
//! ```
//!
//! Nothing was leaking. But two identical-looking call sites had opposite semantics decided
//! by which module they happened to reach, and the type said nothing about which. The first
//! fix flipped `solve` to `false`. That was the weaker fix, and BigMama named why: a default
//! that fails safe is still a default deciding contamination silently — it only changes which
//! forgetful caller gets burned. Joel, the same night: *"unwraps are most of the time
//! idiotic"*, and earlier: *"use constants or enums so you cannot make capitalization type
//! issues. Use rust as it is meant to be used, for predictable behavior."*
//!
//! So the decision is a type with **no `Default` impl**. Every Rust construction site must
//! name a variant or the crate does not build. A new `agent/solve` caller cannot copy the
//! `learn: None` idiom from a sibling call site, because there is no `None` to copy.
//!
//! # What it is guarding
//!
//! #312: six verbatim GitHub issues consolidated into a durable semantic belief that WAS the
//! held-out answer, scoring memorization as capability. Learning on a measurement path is not
//! a tidiness problem — it silently converts a benchmark into a lookup.
//!
//! And the risk got sharper the day this landed, exactly as BigMama predicted: while the
//! consolidation pipeline was unwired, "learning" meant exam text sat inert in episodic. Once
//! it runs, it means the exam text CRYSTALLIZES into a durable semantic belief.
//!
//! # What it is NOT
//!
//! Not an argument against learning. Joel, 2026-08-06: *"We learn during benchmarks and from
//! doing, hearing, seeing… they must learn or what's the point?"* A being learns from her
//! work — that is the whole thesis. This type is about WHO STATES THE INTENT, not whether
//! learning is good. The lesson crosses back; the paper never does.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Does the experience of this run rejoin the living persona's memory?
///
/// Deliberately has **no `Default`** — see the module docs. If you are reaching for one, the
/// question you actually need to answer is "is this run a measurement or is it her life?"
// No ts-rs derive on purpose: this type serializes AS A BOOLEAN, so the TypeScript side sees
// `learn?: boolean` exactly as it always has. Exporting it would emit a binding for a type
// that never appears on the wire — a file no consumer can import and the drift detector would
// have to babysit forever. The discipline this type enforces is a RUST-side discipline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, JsonSchema)]
pub enum LearningPolicy {
    /// Her life. The redacted lesson of this work is admitted to the living persona.
    LearnFromThisWork,
    /// A measurement. The fork is discarded and teaches nothing — #59 isolation, intact.
    DoNotLearn,
}

impl LearningPolicy {
    /// The ONE default in the system, and it exists for exactly one reason: a JSON/CLI caller
    /// can omit a field and no compiler can stop them. Rust callers never reach this — the
    /// missing `Default` impl makes the omission a build error instead.
    ///
    /// It resolves to [`Self::DoNotLearn`] because the omission must fail SAFE: forgetting
    /// costs a lesson, which is recoverable, where the other direction poisons a benchmark,
    /// which is not.
    pub fn wire_default() -> Self {
        Self::DoNotLearn
    }

    /// Whether this run's experience rejoins the living persona.
    pub fn learns(self) -> bool {
        matches!(self, Self::LearnFromThisWork)
    }
}

/// Named spelling accepted on the wire, alongside the historical bool.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum NamedForm {
    LearnFromThisWork,
    DoNotLearn,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WireForm {
    /// `"learn": true` — every caller that existed before this type did.
    Flag(bool),
    /// `"learn": "learn_from_this_work"` — the self-describing spelling.
    Named(NamedForm),
}

impl<'de> Deserialize<'de> for LearningPolicy {
    /// Accepts BOTH the historical `true`/`false` and the named form, so making the Rust side
    /// explicit does not break a single CLI invocation or stored params blob. The type
    /// discipline is for us; the wire stays kind to whoever is typing it.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match WireForm::deserialize(d)? {
            WireForm::Flag(true) | WireForm::Named(NamedForm::LearnFromThisWork) => {
                Self::LearnFromThisWork
            }
            WireForm::Flag(false) | WireForm::Named(NamedForm::DoNotLearn) => Self::DoNotLearn,
        })
    }
}

impl Serialize for LearningPolicy {
    /// Serializes as the historical bool so recorded params, replays and the TypeScript
    /// binding are unchanged by this refactor.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bool(self.learns())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the whole point of the type — an omitted `learn` must never mean
    // LEARN. Pinned as a property of the ONE wire default rather than of each reader, which
    // is the structural difference from the divergent `unwrap_or` pair this replaced.
    #[test]
    fn an_omitted_learn_flag_resolves_to_do_not_learn() {
        assert!(
            !LearningPolicy::wire_default().learns(),
            "the wire default must fail SAFE — a forgotten flag costs a lesson, never a \
             contaminated benchmark (#312)"
        );
    }

    // what this catches: making the Rust side explicit must not break the CLI. Every caller
    // that predates this type spells the field `true`/`false`, and a params blob recorded
    // before today must still replay.
    #[test]
    fn the_historical_bool_spelling_still_deserializes() {
        let learn: LearningPolicy = serde_json::from_str("true").expect("bool true");
        let dont: LearningPolicy = serde_json::from_str("false").expect("bool false");
        assert_eq!(learn, LearningPolicy::LearnFromThisWork);
        assert_eq!(dont, LearningPolicy::DoNotLearn);

        // …and it round-trips back as a bool, so recorded params and the TS binding are
        // untouched by the refactor.
        assert_eq!(serde_json::to_string(&learn).expect("ser"), "true");
        assert_eq!(serde_json::to_string(&dont).expect("ser"), "false");
    }

    // what this catches: the named spelling is the self-describing form a human or a persona
    // would reach for, and it must mean the same thing as the bool it replaces.
    #[test]
    fn the_named_spelling_means_the_same_thing() {
        let learn: LearningPolicy =
            serde_json::from_str("\"learn_from_this_work\"").expect("named learn");
        let dont: LearningPolicy = serde_json::from_str("\"do_not_learn\"").expect("named dont");
        assert_eq!(learn, LearningPolicy::LearnFromThisWork);
        assert_eq!(dont, LearningPolicy::DoNotLearn);
    }
}
