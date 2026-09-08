//! THE BOARD FOLLOWS THE VERDICT — in both directions.
//!
//! One place, for every path that records a SWE verdict (a citizen's `PASS: done`, the
//! tick sweep, an operator `benchmark/swe-grade`): a RESOLVED instance closes its cards
//! and the room hears it; a FAILED instance goes BACK to its holder with the failing
//! tests named, and the card returns to in-progress so she works it again.
//!
//! Why the second half exists: django-11749 (2026-09-07) was graded `resolved=false`
//! with `failed_tests = [test_mutually_exclusive_group_required_options]` at 17:13Z;
//! the verdict reached her experience stream and nothing else. No room line, no card
//! change — she spent the next two hours re-running a repro that "looks good", blind to
//! the one test the grader had already named. A grade is feedback; feedback that never
//! reaches the hands that can act on it is a number in a file.
//!
//! An UNGRADEABLE result writes no verdict and touches nothing here (probe
//! `benchmark.verdict.board_close_skipped` says so at the call site).

use std::sync::Arc;

use uuid::Uuid;

use crate::cognition::swe_bench::SweVerdict;

/// What happened to the board card — the line must say it, never assume it (Astra's
/// review of #3868: a failed board write must not produce a success claim in the room).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardMove {
    Closed,
    ReturnedToHolder,
    Failed(String),
}

/// The line the room hears. Pure so it is tested, not restated.
pub fn verdict_line(verdict: &SweVerdict, holder_name: Option<&str>, moved: &BoardMove) -> String {
    let score = format!(
        "FAIL_TO_PASS {}/{}, PASS_TO_PASS {}/{}",
        verdict.f2p_passed, verdict.f2p_total, verdict.p2p_passed, verdict.p2p_total
    );
    let card = match moved {
        BoardMove::Closed => "The card is closed.".to_string(),
        BoardMove::ReturnedToHolder => "The card is back with you as in-progress: make the \
             failing test pass in your checkout, then 'PASS: done' again."
            .to_string(),
        BoardMove::Failed(e) => format!(
            "The board card could NOT be moved ({e}); it reads as you left it — the grade \
             above still stands."
        ),
    };
    if verdict.resolved {
        return format!("✅ {} RESOLVED — {score}. {card}", verdict.instance_id);
    }
    let at = holder_name.map(|n| format!("@{n} ")).unwrap_or_default(); // unwrap_or: no live holder → an undirected room line, never a bare "@"
    let failing = if verdict.failed_tests.is_empty() {
        String::new()
    } else {
        format!(" — still failing: {}", verdict.failed_tests.join(", "))
    };
    format!(
        "❌ {at}{} graded: not resolved — {score}{failing}. {card}",
        verdict.instance_id
    )
}

/// What a recorded verdict does to the board. Never called for gold or ungradeable runs.
pub async fn follow(verdict: &SweVerdict) {
    let cards = crate::cognition::bench_round::cards_for_instance(&verdict.instance_id);
    if cards.is_empty() {
        return;
    }
    let short = |c: &Uuid| c.to_string().chars().take(8).collect::<String>();
    let Some(airc) = crate::persona::operator_peer::operator_airc() else {
        crate::probe!(
            class = "benchmark.verdict.board_close_failed",
            instance = verdict.instance_id.as_str(),
            card = "-",
            error = "operator self-peer not online",
            "no airc handle to move the board card with"
        );
        return;
    };
    for card in cards {
        let card_id = airc_lib::WorkCardId::from_uuid(card);
        let next = if verdict.resolved {
            airc_lib::CardState::Closed
        } else {
            airc_lib::CardState::InProgress
        };
        let moved = match crate::modules::work::advance_card_state(
            &airc,
            card_id,
            next,
            crate::modules::work::VIA_VERDICT,
            None,
        )
        .await
        {
            Ok(()) if verdict.resolved => {
                crate::probe!(
                    class = "benchmark.verdict.board_closed",
                    instance = verdict.instance_id.as_str(),
                    card = %short(&card),
                    resolved = verdict.resolved,
                    "the board card follows the verdict — closed"
                );
                BoardMove::Closed
            }
            Ok(()) => {
                crate::probe!(
                    class = "benchmark.verdict.returned_to_holder",
                    instance = verdict.instance_id.as_str(),
                    card = %short(&card),
                    failing = %verdict.failed_tests.join(","),
                    "the board card follows the verdict — back to in-progress with the failing tests named"
                );
                BoardMove::ReturnedToHolder
            }
            Err(e) => {
                crate::probe!(
                    class = "benchmark.verdict.board_close_failed",
                    instance = verdict.instance_id.as_str(),
                    card = %short(&card),
                    error = %e,
                    "the verdict is recorded but the board card could not be moved — it reads as she left it until the next pass"
                );
                BoardMove::Failed(e)
            }
        };
        say_in_card_room(verdict, card, &airc, &moved).await;
    }
}

/// The room hears the verdict, addressed to the holder. Authored by a live citizen who is
/// NOT the holder (a citizen's inbound stream skips her own lines, so a mention she
/// authored would be dropped), else by the operator.
async fn say_in_card_room(
    verdict: &SweVerdict,
    card: Uuid,
    operator: &Arc<airc_lib::Airc>,
    moved: &BoardMove,
) {
    let short = card.to_string().chars().take(8).collect::<String>();
    let Some(room) = crate::cognition::bench_round::room_for_card(card) else {
        crate::probe!(
            class = "benchmark.verdict.line_not_posted",
            instance = verdict.instance_id.as_str(),
            card = %short,
            error = "no room for card",
            "the verdict has no room to be said in"
        );
        return;
    };
    let registry = crate::persona::PersonaAircRuntimeRegistry::try_global();
    // Three answers, kept apart (Astra's review): the board says WHO holds it, the board
    // says NOBODY holds it (a lapsed or unclaimed card — do not resurrect the dispatch
    // assignee over the board's word), or the board could not be read at all (then the
    // dispatch-time assignee is the best historical guess, and is labelled so).
    let (holder, historical) = match holder_of(card, room, registry.as_ref()).await {
        HolderRead::Held(h) => (Some(h), false),
        HolderRead::Unheld => (None, false),
        HolderRead::Unavailable(why) => {
            crate::probe!(
                class = "benchmark.verdict.holder_unavailable",
                instance = verdict.instance_id.as_str(),
                card = %short,
                why = %why,
                "the live board could not be read — addressing the dispatch-time assignee as a historical guess"
            );
            (crate::cognition::bench_round::card_assignee(card), true)
        }
    };
    // A holder is a mesh citizen, not necessarily a runtime on this node.
    // Use the same durable identity lookup as the work board; presence and
    // local residency cannot decide whether a teammate is named in feedback.
    let holder_name = match holder {
        Some(id) => {
            let alias = match operator.peer_alias(airc_core::PeerId::from_uuid(id)).await {
                Ok(alias) => alias,
                Err(error) => {
                    crate::probe!(
                        class = "benchmark.verdict.holder_name_unavailable",
                        instance = verdict.instance_id.as_str(),
                        card = %short,
                        holder = %id,
                        error = %error,
                        "identity lookup failed — the verdict still addresses the holder by id"
                    );
                    None
                }
            };
            Some(holder_label(id, alias.as_deref(), historical))
        }
        None => None,
    };
    let line = verdict_line(verdict, holder_name.as_deref(), moved);
    let voice: Arc<airc_lib::Airc> = registry
        .as_ref()
        .and_then(|r| r.any_live_citizen_other_than(holder))
        .map(|rt| rt.airc().clone())
        .unwrap_or_else(|| operator.clone()); // unwrap_or: no live citizen other than the holder → the operator speaks (never the holder herself)
    match crate::persona::airc_citizen::publish_text_in_room(&voice, room, &line).await {
        Ok(_) => crate::probe!(
            class = "benchmark.verdict.line_posted",
            instance = verdict.instance_id.as_str(),
            card = %short,
            room = %room.to_string().chars().take(8).collect::<String>(),
            holder = %holder_name.unwrap_or_else(|| "-".into()), // unwrap_or: "-" = no live holder, a legible absence in the probe
            resolved = verdict.resolved,
            "the room heard the verdict"
        ),
        Err(e) => crate::probe!(
            class = "benchmark.verdict.line_not_posted",
            instance = verdict.instance_id.as_str(),
            card = %short,
            error = %e,
            "the verdict is recorded but the room did not hear it"
        ),
    }
}

/// Keep an unnamed holder addressable and a historical guess visibly historical.
fn holder_label(id: Uuid, alias: Option<&str>, historical: bool) -> String {
    let name = alias
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| id.to_string()); // unwrap_or: unknown identity stays addressable, never disappears from feedback
    if historical {
        format!("{name} (dispatch assignee; live board unavailable)")
    } else {
        name
    }
}

/// What the live board says about the card's holder — three distinct answers.
#[derive(Debug, PartialEq, Eq)]
pub enum HolderRead {
    Held(Uuid),
    /// The board was read and no one holds the card (lapsed or unclaimed).
    Unheld,
    /// The board could not be read; the reason names the joint that failed.
    Unavailable(&'static str),
}

/// Who holds the card NOW, from the live board (`card.owner` alone is never cleared on
/// reopen, so the hold is decided by `card_holder::hold_of`). Read through any live
/// citizen's airc — citizens are seated in the run room; the operator may not be. A clock
/// that cannot be read is an unavailable read, never a made-up timestamp: zero would sort
/// before every lease expiry and call a lapsed hold live.
async fn holder_of(
    card: Uuid,
    room: Uuid,
    registry: Option<&crate::persona::PersonaAircRuntimeRegistry>,
) -> HolderRead {
    let Some(rt) = registry.and_then(|r| r.any_live_citizen()) else {
        return HolderRead::Unavailable("no live citizen to read the board through");
    };
    let airc = rt.airc().clone();
    let Ok(set) = airc.subscription_set().await else {
        return HolderRead::Unavailable("subscription set unreadable");
    };
    let Some(room) = set
        .all()
        .map(|sub| sub.as_room())
        .find(|r| r.channel.as_uuid() == room)
    else {
        return HolderRead::Unavailable("reading citizen is not subscribed to the card's room");
    };
    let Ok(board) = airc.work_board_in(&room).await else {
        return HolderRead::Unavailable("board read failed");
    };
    let board = board.snapshot();
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return HolderRead::Unavailable("clock before the epoch");
    };
    let now_ms = now.as_millis() as u64;
    let Some(found) = board.cards.iter().find(|c| c.card_id.as_uuid() == card) else {
        return HolderRead::Unavailable("card not on the board");
    };
    match crate::persona::card_holder::hold_of(found, now_ms) {
        crate::persona::card_holder::Hold::Held => match found.owner {
            Some(o) => HolderRead::Held(o.as_uuid()),
            None => HolderRead::Unheld,
        },
        crate::persona::card_holder::Hold::Lapsed
        | crate::persona::card_holder::Hold::Unclaimed => HolderRead::Unheld,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Regression: remote holders have no local runtime. Missing identity must
    // not erase their address, or erase the uncertainty of an old assignment.
    #[test]
    fn remote_holder_feedback_preserves_identity_and_historical_uncertainty() {
        let id = Uuid::from_u128(42);
        let v = failed_11749();
        let named = holder_label(id, Some("Remote teammate"), false);
        assert!(verdict_line(&v, Some(&named), &BoardMove::ReturnedToHolder)
            .contains("@Remote teammate "));
        for alias in [None, Some(""), Some("  ")] {
            let unknown = holder_label(id, alias, true);
            let line = verdict_line(&v, Some(&unknown), &BoardMove::ReturnedToHolder);
            assert!(line.contains(&format!(
                "@{id} (dispatch assignee; live board unavailable)"
            )));
            assert!(line.contains("still failing: test_mutually_exclusive_group_required_options"));
        }
    }

    // what this catches: a failed grade that names no test, or no holder, or reads like a
    // score instead of an instruction (django-11749, 2026-09-07: the failing test sat in
    // the verdict file for two hours while the holder re-ran a repro that "looked good").
    fn failed_11749() -> SweVerdict {
        SweVerdict {
            instance_id: "django__django-11749".into(),
            resolved: false,
            f2p_passed: 0,
            f2p_total: 1,
            p2p_passed: 34,
            p2p_total: 34,
            gate_ok: true,
            failed_tests: vec!["test_mutually_exclusive_group_required_options".into()],
            ..Default::default()
        }
    }

    #[test]
    fn a_failed_verdict_names_the_holder_and_the_failing_tests_and_hands_the_card_back() {
        let v = failed_11749();
        let line = verdict_line(&v, Some("Joaquin"), &BoardMove::ReturnedToHolder);
        assert!(
            line.starts_with("❌ @Joaquin django__django-11749 graded: not resolved"),
            "{line}"
        );
        assert!(line.contains("still failing: test_mutually_exclusive_group_required_options"));
        assert!(line.contains("back with you as in-progress"));
        // No live holder: still a room line, never a panic, never a bare "@".
        let anon = verdict_line(&v, None, &BoardMove::ReturnedToHolder);
        assert!(anon.starts_with("❌ django__django-11749"), "{anon}");
        let resolved = SweVerdict {
            resolved: true,
            f2p_passed: 1,
            ..v
        };
        let ok = verdict_line(&resolved, Some("Joaquin"), &BoardMove::Closed);
        assert!(
            ok.starts_with("✅ django__django-11749 RESOLVED")
                && ok.ends_with("The card is closed."),
            "{ok}"
        );
    }

    // what this catches: a failed board write producing a success claim in the room
    // (Astra's review of #3868) — the grade must still be said, the move must be named
    // as failed, and neither "closed" nor "back with you" may appear.
    #[test]
    fn a_failed_board_move_is_named_in_the_line_and_never_claims_success() {
        let v = failed_11749();
        let failed = BoardMove::Failed("state machine refused Review → InProgress".into());
        let line = verdict_line(&v, Some("Joaquin"), &failed);
        assert!(
            line.contains("still failing: test_mutually_exclusive_group_required_options"),
            "{line}"
        );
        assert!(
            line.contains("could NOT be moved (state machine refused Review → InProgress)"),
            "{line}"
        );
        assert!(
            !line.contains("back with you") && !line.contains("is closed"),
            "{line}"
        );
        let resolved = SweVerdict {
            resolved: true,
            f2p_passed: 1,
            ..v
        };
        let line = verdict_line(&resolved, None, &failed);
        assert!(
            line.starts_with("✅")
                && line.contains("could NOT be moved")
                && !line.contains("is closed"),
            "{line}"
        );
    }
}
