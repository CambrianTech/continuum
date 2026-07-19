//! Tool conformance harness (#163) — proves, BY CONSTRUCTION, that every tool a
//! persona is OFFERED is actually usable by a model.
//!
//! ## Why this exists
//!
//! The persona's hands are a moat only if they RELIABLY work. But the way we've
//! learned which tools fail an AI has been a hand-authored defect list (mined from
//! live captures: glob rejected, short-UUID rejected, unknown-tool silent), and
//! that list DRIFTS — several entries were already fixed by the time they were
//! read. A hand-maintained list of "tools an AI can't use" is exactly the thing a
//! machine should maintain.
//!
//! So this is the machine's half: it enumerates the NATIVE tool set (the commands
//! a persona is actually offered — `native == true`, derived from the registry,
//! never a hand-kept list) and checks each against the AI-usability floor. The
//! test asserts the audit is empty, so a NEW tool that ships undiscoverable or
//! unlearnable fails CI the moment it lands — the list can't drift because it
//! isn't a list, it's a computation over the live registry.
//!
//! ## The floor (this slice — static/discoverability invariants)
//!
//! A model can only use a tool it can (1) NAME in its own paradigm, (2) know WHEN
//! to reach for (a real description), and (3) learn to FILL IN (a renderable input
//! contract). Those are the three rules here. Behavioral exams — dispatch garbage
//! → fail LOUD, id params accept short forms (#164) — are the next slice; they
//! need a live executor, so they layer on top of this static floor.

use crate::sdk_codegen::{command_registry, AccessLevel};

/// The minimum a model-facing DESCRIPTION needs to orient a reasoner. Shorter than
/// this is effectively undocumented — not a sentence, just a label.
const MIN_DESCRIPTION_CHARS: usize = 12;

/// One way a tool fails the AI-usability floor. Carries the tool, which rule it
/// broke, and a persona-paradigm explanation of the gap (this doubles as the fix
/// instruction when the CI gate fails).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolConformanceViolation {
    pub tool: String,
    pub rule: &'static str,
    pub detail: String,
}

impl std::fmt::Display for ToolConformanceViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "  [{}] {} — {}", self.rule, self.tool, self.detail)
    }
}

/// Whether a name is one a model can reliably EMIT: lowercase ascii, digits, and
/// the path/segment punctuation the executor maps (`/`, `_`, `-`). A capital or a
/// space is a name a model will mangle, so the tool would be unreachable.
fn name_is_model_emittable(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '/' | '_' | '-'))
}

/// The count of tools the audit examines (the AiSafe surface). Exposed so the
/// gate can assert NON-VACUITY: if a registry/filter regression ever made this
/// zero, an empty violation list would pass silently — the gate would be green
/// because it checked nothing. The floor is deliberately loose (well under the
/// live ~150) so it never turns brittle as the surface grows or shrinks.
pub fn audited_tool_count() -> usize {
    command_registry()
        .into_iter()
        .filter(|d| d.access_level == AccessLevel::AiSafe)
        .count()
}

/// Audit every AiSafe tool against the AI-usability floor. Returns EVERY violation
/// (not just the first) so a fix pass sees the whole surface at once; empty ==
/// conformant. Pure over the (after-boot-immutable) registry — no I/O, no dispatch.
pub fn audit_tool_conformance() -> Vec<ToolConformanceViolation> {
    let mut violations = Vec::new();
    // Scope = the whole AiSafe surface, not just the bounded NATIVE set. A persona
    // is offered the native set directly, but reaches EVERY AiSafe command by name
    // through `commands/catalog` + `commands/help` — so any AiSafe command that is
    // undiscoverable or unlearnable is a hand she can pick up but can't use. The
    // native subset is included (native ⊂ AiSafe).
    for d in command_registry()
        .into_iter()
        .filter(|d| d.access_level == AccessLevel::AiSafe)
    {
        // 1. NAME the model can emit — else the tool is unreachable.
        if !name_is_model_emittable(d.name) {
            violations.push(ToolConformanceViolation {
                tool: d.name.to_string(),
                rule: "name-emittable",
                detail: format!(
                    "'{}' has characters a model can't reliably emit — use lowercase a-z 0-9 and / _ -",
                    d.name
                ),
            });
        }

        // 2. DISCOVERABLE — a real description, so a persona knows WHEN to use it.
        let desc_len = d.description.trim().chars().count();
        if desc_len < MIN_DESCRIPTION_CHARS {
            violations.push(ToolConformanceViolation {
                tool: d.name.to_string(),
                rule: "description-present",
                detail: format!(
                    "description is {desc_len} chars (< {MIN_DESCRIPTION_CHARS}) — a persona \
                     can't learn when to reach for it; add a one-line `///` doc"
                ),
            });
        }

        // 3. LEARNABLE input — the schema is either Null (genuinely no params) or a
        //    JSON object `commands/help` can render into a fill-in-the-blanks call.
        //    A non-object, non-null schema is a contract a model can't complete.
        if !d.params_schema.is_null() && !d.params_schema.is_object() {
            violations.push(ToolConformanceViolation {
                tool: d.name.to_string(),
                rule: "params-learnable",
                detail: "params schema is neither null (no params) nor an object — not a \
                         fill-in-the-blanks contract a model can complete"
                    .to_string(),
            });
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (#163): a NATIVE tool that ships undiscoverable (no/short
    // description), unnameable (a caps/space a model can't emit), or unlearnable
    // (a non-object params schema) — the AI-usability floor. This is the living,
    // self-updating replacement for the hand-authored tool-defect list: a new
    // tool that violates the floor fails HERE the moment it lands, and the message
    // names the exact tool + fix. The audit runs over the live registry, so it can
    // never drift the way a checked-in list does.
    #[test]
    fn ai_safe_tools_meet_the_ai_usability_floor() {
        // Non-vacuity: the audit must actually have examined the surface. A green
        // result on zero tools would be a silently-broken gate, not conformance.
        let audited = audited_tool_count();
        assert!(
            audited >= 40,
            "conformance audit examined only {audited} AiSafe tools — expected the full \
             surface (~150). The registry filter is broken; the gate is checking nothing."
        );

        let violations = audit_tool_conformance();
        assert!(
            violations.is_empty(),
            "{} of {audited} AiSafe tool(s) fail the AI-usability floor — a persona can reach \
             them by name but cannot reliably use them:\n{}",
            violations.len(),
            violations
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}
