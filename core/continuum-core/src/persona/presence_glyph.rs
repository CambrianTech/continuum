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

/// A thought line, clipped to `max_chars` with an ellipsis when longer.
pub fn thought_line(thought: &str, max_chars: usize) -> String {
    let clipped: String = thought.chars().take(max_chars).collect();
    let more = if thought.chars().count() > max_chars { "…" } else { "" };
    format!("{THOUGHT} {clipped}{more}")
}

/// An act receipt line: `⚙ verb object ✓`.
pub fn act_line(verb: &str, object: &str, ok: bool) -> String {
    format!("{ACT} {verb} {object} {}", if ok { OK } else { FAIL })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the plane sniff drifting from the writer — a thought
    // or act line the writer emits must read as presence at the head of the
    // line, and speech must not.
    #[test]
    fn what_the_writer_emits_the_sniff_classes_as_presence() {
        assert!(is_presence_line(&thought_line("let me look at fields.py", 240)));
        assert!(is_presence_line(&act_line("code/read", "swe/x.py", true)));
        assert!(is_presence_line("  ⚙ code/edit swe/x.py ✗"));
        assert!(!is_presence_line("Joel here — which card do you hold?"));
        assert_eq!(thought_line("abcdef", 3), "💭 abc…");
        assert_eq!(act_line("code/edit", "a.py", false), "⚙ code/edit a.py ✗");
    }
}
