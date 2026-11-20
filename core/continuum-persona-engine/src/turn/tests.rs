//! A turn decodes only what her blocks do not hold, and never decodes against stale blocks.

use super::*;

#[derive(Debug, PartialEq)]
struct Under {
    model: &'static str,
    genes: &'static str,
    epoch: u64,
    media: &'static str,
}

const BASE: Under = Under { model: "35b", genes: "", epoch: 1, media: "" };

fn h<'a>(basis: &'a Under, tokens: &'a [i32]) -> History<'a, Under> {
    History { basis, tokens }
}

// what this catches: re-prefilling her whole history every turn (the old scheduler's
// memory_seq_rm-per-request; 4.4-50 s re-prefills measured on the 35B). An extending prompt under the
// same basis decodes only its tail.
#[test]
fn an_extending_prompt_prefills_only_its_tail() {
    assert_eq!(plan(h(&BASE, &[1, 2, 3]), h(&BASE, &[1, 2, 3, 4, 5])), Prefill::Append { from: 3 });
}

// what this catches: sampling from logits no position kept. A prompt that adds nothing re-decodes her
// last token so its logits exist.
#[test]
fn a_prompt_that_adds_nothing_re_decodes_her_last_token() {
    assert_eq!(plan(h(&BASE, &[1, 2, 3]), h(&BASE, &[1, 2, 3])), Prefill::Append { from: 2 });
}

// what this catches: decoding a rewritten conversation against blocks that hold another history. A
// diverging, shorter or empty prompt, and a mind with nothing held, start a newer epoch.
#[test]
fn a_prompt_that_does_not_extend_her_history_restarts() {
    assert_eq!(plan(h(&BASE, &[1, 2, 3]), h(&BASE, &[1, 9, 3, 4])), Prefill::Restart, "diverges");
    assert_eq!(plan(h(&BASE, &[1, 2, 3]), h(&BASE, &[1, 2])), Prefill::Restart, "shorter than she holds");
    assert_eq!(plan(h(&BASE, &[1, 2, 3]), h(&BASE, &[])), Prefill::Restart, "nothing to sample from");
    assert_eq!(plan(h(&BASE, &[]), h(&BASE, &[1, 2])), Prefill::Restart, "nothing held yet");
}

// what this catches: reusing blocks computed under another basis because the token ids match (Codex
// on the serving slice). Another model, adapter set, epoch or media identity restarts.
#[test]
fn the_same_tokens_under_another_basis_restart() {
    let tokens = [1, 2, 3];
    let longer = [1, 2, 3, 4];
    for other in [
        Under { model: "4b", ..BASE },
        Under { genes: "rust-review:1.0", ..BASE },
        Under { epoch: 2, ..BASE },
        Under { media: "sha256:ab12", ..BASE },
    ] {
        assert_eq!(plan(h(&BASE, &tokens), h(&other, &longer)), Prefill::Restart, "{other:?}");
    }
}
