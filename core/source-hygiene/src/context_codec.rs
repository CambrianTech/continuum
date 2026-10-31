//! **A mind's context has one codec, `continuum_context`, and nothing works around it.**
//!
//! Joel, 2026-10-25: "make this more systematic in positron just like KV caching and
//! collapsing to prevent this kind of thing"; "Code it together so we don't forget". The view
//! (`continuum_context::view::View`) is the one owner of what she perceives: keyed items that
//! only append within an epoch, levels of detail generated from each item's shape
//! (`continuum_context::levels`), admission instead of turnovers, a restart order measured per
//! mind (`continuum_context::motion`). On the M5 (canary a59c2bf15) three owners each decided a
//! piece of it, and Aiko read one 51,827-char file whole 9 times in 43 acts.
//!
//! These ratchets count the ways around the codec. Each may only fall
//! (docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md, "one codec for every layer", has the
//! recipe for adding a layer the right way):
//! - `prompt_messages_outside_the_view`: a prompt message built by hand in the prompt assembly.
//!   A layer is a `Source`, and the view renders it.
//! - `collapse_outside_the_view`: collapsing what she sees anywhere but a view start. Working
//!   memory bounds RAM by handle; the view decides what she reads.
//! - `hand_churn_labels`: ordering sources by a label (`churn_of`) instead of measured motion.
//! - sizing what she sees outside the one allocation is `budget_discipline::SelfSized`'s class.

use super::{sites, SourceFile, SourceRule, Violation};

/// The prompt assembly, where every message must come from the view or the tail's renderer.
const PROMPT_ASSEMBLY: &str = "cognition/llm_deliberation_faculty/prompt/";

struct PromptMessagesOutsideTheView;
struct CollapseOutsideTheView;
struct HandChurnLabels;

impl SourceRule for PromptMessagesOutsideTheView {
    fn name(&self) -> &'static str {
        "prompt_messages_outside_the_view"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if !file.rel.starts_with(PROMPT_ASSEMBLY) {
            return Vec::new();
        }
        sites(file, self.name(), |code| {
            (code.contains("ChatMessage::text(") || code.contains("ChatMessage::tool_results(")).then(|| "a message pushed by hand".to_owned())
        })
    }
}

impl SourceRule for CollapseOutsideTheView {
    fn name(&self) -> &'static str {
        "collapse_outside_the_view"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        sites(file, self.name(), |code| {
            (code.contains(".collapse_over(") || code.contains("collapse_results_over_cap()")).then(|| "a collapse outside the view".to_owned())
        })
    }
}

impl SourceRule for HandChurnLabels {
    fn name(&self) -> &'static str {
        "hand_churn_labels"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        sites(file, self.name(), |code| code.contains("churn_of(").then(|| "an order by label".to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    fn hold(rule: &dyn SourceRule, instead: &str) {
        let violations = scan(&[rule]);
        let count = violations.len();
        crate::ratchet::hold(rule.name(), count, || {
            format!(
                "{} rose to {count}. {instead} The recipe: docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md, \
                 \"How to add a layer\".\nFirst {} of {count} (not necessarily new):\n{}",
                rule.name(),
                count.min(20),
                violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
            )
        });
    }

    // what this catches: a prompt block pushed by hand, outside the view's append-only epoch,
    // which shifts everything after it and re-prefills it every act.
    #[test]
    fn prompt_messages_outside_the_view_never_increase() {
        hold(&PromptMessagesOutsideTheView, "Make the block a continuum_context Source and add it to the view's sources.");
    }

    // what this catches: a second allocator deciding what she sees (working memory collapsed her
    // newest file read the act after she read it, so she read it again).
    #[test]
    fn collapse_outside_the_view_never_increases() {
        hold(&CollapseOutsideTheView, "Give the item a summary level (continuum_context::levels) and let the view's start choose it.");
    }

    // what this catches: a restart order by label; the roster was labelled stable and changed with
    // every room switch (5 of 12 turnovers kept 3-6%).
    #[test]
    fn hand_churn_labels_never_increase() {
        hold(&HandChurnLabels, "The view orders a start by measured motion (continuum_context::motion).");
    }

    // what this catches: the predicates that make the rules useful rather than noise.
    #[test]
    fn each_rule_catches_its_shape_and_spares_its_definition() {
        let count = |rule: &dyn SourceRule, rel: &str, src: &str| rule.check(&SourceFile::for_test(rel, src)).len();
        let prompt = "cognition/llm_deliberation_faculty/prompt/messages.rs";
        assert_eq!(count(&PromptMessagesOutsideTheView, prompt, "messages.push(ChatMessage::text(\"user\", fact));"), 1);
        assert_eq!(count(&PromptMessagesOutsideTheView, "cognition/other.rs", "ChatMessage::text(\"user\", x)"), 0);
        assert_eq!(count(&CollapseOutsideTheView, "cognition/wm.rs", "self.collapse_results_over_cap();"), 1);
        assert_eq!(count(&CollapseOutsideTheView, "cognition/wm.rs", "pub fn collapse_over(&mut self, cap: usize) -> Collapsed {"), 0);
        assert_eq!(count(&HandChurnLabels, "cognition/a.rs", "churn_of(&block.faculty) < PromptChurn::Claims"), 1);
        assert_eq!(count(&HandChurnLabels, "cognition/a.rs", "pub(super) fn churn_of(faculty: &str) -> PromptChurn {"), 0);
    }
}
