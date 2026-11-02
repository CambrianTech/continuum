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
/// Accepted, not finished — a handle came back and the work is still running.
/// Its own mark because a tick here is a lie about work that has not happened.
pub const RUNNING: &str = "⋯";
/// The substrate could not read the result. Not a failure, and not a success:
/// an absence of information, which must look like one.
pub const UNKNOWN: &str = "·";

/// Presence plane: a line whose head is a thought or an act receipt — state
/// radiated while working, not a message to anyone.
pub fn is_presence_line(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with(THOUGHT) || t.starts_with(ACT)
}

/// What she said as she acted, as a presence line: `💭 <her words>`, whole. Her words, never
/// her reasoning: what she thinks stays hers (docs/architecture/PRIVACY-OF-THOUGHT.md, sink 6),
/// reaching her own next turn through working memory and a human through the UI's act
/// receipt. Never clipped: a room line is not a budget, and her words are hers to size.
pub fn said_line(said: &str) -> String {
    format!("{THOUGHT} {said}")
}

/// An act receipt line: `⚙ verb object ✓`.
///
/// THREE marks, not two (card f6c50a49). A tick used to be printed for every act
/// whose transport did not error — so `code/run` returning `ok: false` with the
/// compiler's stderr, and `code/shell` handing back a still-RUNNING handle, both
/// read as success in the room. An accepted call is not a finished one, and a
/// result nobody could decode is not a success either.
pub fn act_line(verb: &str, object: &str, verdict: continuum_kernel::sdk_codegen::ActVerdict) -> String {
    let mark = match verdict {
        v if v.failed() => FAIL,
        v if v.running() => RUNNING,
        // Unprojected keeps the old tick: the command never opted in, so this line
        // says exactly what it said before. Undecodable deliberately does NOT get a
        // tick — the substrate could not read the result and must not claim one.
        continuum_kernel::sdk_codegen::ActVerdict::Undecodable => UNKNOWN,
        _ => OK,
    };
    format!("{ACT} {verb} {object} {mark}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_kernel::sdk_codegen::{ActVerdict, ToolVerdict};

    // what this catches: the plane sniff drifting from the writer — a thought
    // or act line the writer emits must read as presence at the head of the
    // line, and speech must not.
    #[test]
    fn what_the_writer_emits_the_sniff_classes_as_presence() {
        assert!(is_presence_line(&said_line("let me look at fields.py")));
        assert!(is_presence_line(&act_line(
            "code/read",
            "swe/x.py",
            ActVerdict::Declared(ToolVerdict::Succeeded)
        )));
        assert!(is_presence_line("  ⚙ code/edit swe/x.py ✗"));
        assert!(!is_presence_line("Joel here — which card do you hold?"));
        assert_eq!(said_line("checking the parser first"), "💭 checking the parser first");
        assert_eq!(
            act_line("code/edit", "a.py", ActVerdict::Declared(ToolVerdict::Failed)),
            "⚙ code/edit a.py ✗"
        );

        // what this catches (card f6c50a49): the two states a BOOL could not hold.
        // A shell handed back still running, and a result the substrate could not
        // decode, both used to print the success tick.
        assert_eq!(
            act_line("code/shell", "cargo test", ActVerdict::Declared(ToolVerdict::Running)),
            "⚙ code/shell cargo test ⋯",
            "accepted is not finished"
        );
        assert_eq!(
            act_line("code/run", "x.py", ActVerdict::Undecodable),
            "⚙ code/run x.py ·",
            "a result nobody could read must not claim success"
        );
        // A command that never opted in is UNCHANGED — still a tick.
        assert_eq!(
            act_line("code/read", "a.py", ActVerdict::Unprojected),
            "⚙ code/read a.py ✓"
        );
    }
}
