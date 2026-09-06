//! The PRESENCE-PLANE register: the one leading glyph that classes a
//! citizen's line without parsing its body. `💭` is a working thought, `⚙` an
//! act receipt (`✓`/`✗` its outcome). It grew as pidgin between citizens
//! (Joel, 2026-09-04: "agents love emojis … natural compressed language …
//! reminds me of pidgin") — and it stays THEIRS: "let the personas control and
//! design that plane; we don't freeze the vocabulary, they evolve it, and it
//! gets learned as genome — a trainable register, not an agent-authored spec."
//!
//! So this module is the PROJECTION SEAM, not the semantics: glyph → avatar
//! animation → attention hint, one place every reader sniffs through (the
//! attention filter, the digest collapse, the resume block, the avatar). The
//! two constants below are what the citizens converged on so far, named here
//! so the writer and the readers agree on bytes. Do not grow a closed enum of
//! meanings here; the register's next home is data the personas evolve (a
//! per-node register in state, consolidated into genome), with this seam
//! reading it.

/// A working thought, spoken as she acts.
pub const THOUGHT: &str = "💭";
/// An act receipt: the verb, its object, and the outcome mark.
pub const ACT: &str = "⚙";
/// Outcome marks on an act receipt.
pub const OK: &str = "✓";
pub const FAIL: &str = "✗";

/// Presence plane: a line whose head is a thought or an act receipt — state
/// radiated while working, not a message to anyone.
pub fn is_presence_line(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with(THOUGHT) || t.starts_with(ACT)
}

/// A thought line within `max_chars`. A long thought keeps its HEAD (what it is
/// about) and its TAIL (where it got to), with an ellipsis between.
///
/// Glass-boxed 2026-09-06 (Atlas, django-16899): the receipt was the first 240
/// characters of a stock-take that opened "Let me carefully parse where I actually
/// am…" and ended "so the fix is: include the index in the label at checks.py". Only
/// the opening reached the room, the store, and the next turn's resume block — every
/// turn re-oriented and the conclusion died at the writer. The tail is the part the
/// next turn needs; the head is the part a reader needs to know what it is about.
pub fn thought_line(thought: &str, max_chars: usize) -> String {
    let n = thought.chars().count();
    if n <= max_chars {
        return format!("{THOUGHT} {thought}");
    }
    // Head a third, tail the rest — the conclusion is worth more than the opening.
    let head_chars = max_chars / 3;
    let tail_chars = max_chars.saturating_sub(head_chars);
    let head: String = thought.chars().take(head_chars).collect();
    let tail: String = thought.chars().skip(n - tail_chars).collect();
    format!("{THOUGHT} {head}… {tail}")
}

/// An act receipt line: `⚙ verb object ✓`.
pub fn act_line(verb: &str, object: &str, ok: bool) -> String {
    format!("{ACT} {verb} {object} {}", if ok { OK } else { FAIL })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Atlas, django-16899, 2026-09-06): a long stock-take receipt
    // that keeps only its opening — the conclusion at the end is what the next turn
    // resumes from, and it must survive the writer.
    #[test]
    fn a_long_thought_keeps_its_conclusion_at_the_writer() {
        let thought = format!(
            "Let me carefully parse where I actually am right now. {} So the fix is: include the index in the label at checks.py:1040.",
            "First, the ground truth from my own board and workspace. ".repeat(8)
        );
        let line = thought_line(&thought, 240);
        assert!(line.starts_with("💭 Let me carefully parse"), "{line}");
        assert!(line.ends_with("include the index in the label at checks.py:1040."), "{line}");
        assert!(line.contains("… "), "head and tail joined by an ellipsis: {line}");
        assert!(line.chars().count() <= 240 + 4, "within budget: {}", line.chars().count());
    }

    // what this catches: the plane sniff drifting from the writer — a thought
    // or act line the writer emits must read as presence at the head of the
    // line, and speech must not.
    #[test]
    fn what_the_writer_emits_the_sniff_classes_as_presence() {
        assert!(is_presence_line(&thought_line("let me look at fields.py", 240)));
        assert!(is_presence_line(&act_line("code/read", "swe/x.py", true)));
        assert!(is_presence_line("  ⚙ code/edit swe/x.py ✗"));
        assert!(!is_presence_line("Joel here — which card do you hold?"));
        assert_eq!(thought_line("abcdef", 3), "💭 a… ef");
        assert_eq!(act_line("code/edit", "a.py", false), "⚙ code/edit a.py ✗");
    }
}
