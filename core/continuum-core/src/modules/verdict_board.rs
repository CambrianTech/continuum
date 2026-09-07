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

/// The line the room hears. Pure so it is tested, not restated.
pub fn verdict_line(verdict: &SweVerdict, holder_name: Option<&str>) -> String {
    if verdict.resolved {
        return format!(
            "✅ {} RESOLVED — FAIL_TO_PASS {}/{}, PASS_TO_PASS {}/{}. The card is closed.",
            verdict.instance_id,
            verdict.f2p_passed,
            verdict.f2p_total,
            verdict.p2p_passed,
            verdict.p2p_total
        );
    }
    let at = holder_name.map(|n| format!("@{n} ")).unwrap_or_default();
    let failing = if verdict.failed_tests.is_empty() {
        String::new()
    } else {
        format!(" — still failing: {}", verdict.failed_tests.join(", "))
    };
    format!(
        "❌ {at}{} graded: not resolved — FAIL_TO_PASS {}/{}, PASS_TO_PASS {}/{}{}. The card \
         is back with you as in-progress: make the failing test pass in your checkout, then \
         'PASS: done' again.",
        verdict.instance_id,
        verdict.f2p_passed,
        verdict.f2p_total,
        verdict.p2p_passed,
        verdict.p2p_total,
        failing
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
        match crate::modules::work::advance_card_state(
            &airc,
            card_id,
            next,
            crate::modules::work::VIA_VERDICT,
            None,
        )
        .await
        {
            Ok(()) if verdict.resolved => crate::probe!(
                class = "benchmark.verdict.board_closed",
                instance = verdict.instance_id.as_str(),
                card = %short(&card),
                resolved = verdict.resolved,
                "the board card follows the verdict — closed"
            ),
            Ok(()) => crate::probe!(
                class = "benchmark.verdict.returned_to_holder",
                instance = verdict.instance_id.as_str(),
                card = %short(&card),
                failing = %verdict.failed_tests.join(","),
                "the board card follows the verdict — back to in-progress with the failing tests named"
            ),
            Err(e) => crate::probe!(
                class = "benchmark.verdict.board_close_failed",
                instance = verdict.instance_id.as_str(),
                card = %short(&card),
                error = %e,
                "the verdict is recorded but the board card could not be moved — it reads as she left it until the next pass"
            ),
        }
        say_in_card_room(verdict, card, &airc).await;
    }
}

/// The room hears the verdict, addressed to the holder. Authored by a live citizen who is
/// NOT the holder (a citizen's inbound stream skips her own lines, so a mention she
/// authored would be dropped), else by the operator.
async fn say_in_card_room(verdict: &SweVerdict, card: Uuid, operator: &Arc<airc_lib::Airc>) {
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
    let holder = holder_of(card, room, registry.as_ref())
        .await
        .or_else(|| crate::cognition::bench_round::card_assignee(card));
    let holder_name = holder
        .and_then(|h| registry.as_ref().and_then(|r| r.get(h)))
        .map(|rt| rt.agent_name().to_string());
    let line = verdict_line(verdict, holder_name.as_deref());
    let voice: Arc<airc_lib::Airc> = registry
        .as_ref()
        .and_then(|r| r.any_live_citizen_other_than(holder))
        .map(|rt| rt.airc().clone())
        .unwrap_or_else(|| operator.clone());
    match crate::persona::airc_citizen::publish_text_in_room(&voice, room, &line).await {
        Ok(_) => crate::probe!(
            class = "benchmark.verdict.line_posted",
            instance = verdict.instance_id.as_str(),
            card = %short,
            room = %room.to_string().chars().take(8).collect::<String>(),
            holder = %holder_name.unwrap_or_else(|| "-".into()),
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

/// Who holds the card NOW, from the live board (a lapsed or unclaimed card has no holder;
/// `card.owner` alone is never cleared on reopen). Read through any live citizen's airc —
/// citizens are seated in the run room; the operator may not be.
async fn holder_of(
    card: Uuid,
    room: Uuid,
    registry: Option<&crate::persona::PersonaAircRuntimeRegistry>,
) -> Option<Uuid> {
    let rt = registry?.any_live_citizen()?;
    let airc = rt.airc().clone();
    let set = airc.subscription_set().await.ok()?;
    let room = set.all().map(|sub| sub.as_room()).find(|r| r.channel.as_uuid() == room)?;
    let board = airc.work_board_in(&room).await.ok()?.snapshot();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0); // unwrap_or: a clock before 1970 reads every hold as lapsed, which is the safe side
    let found = board.cards.iter().find(|c| c.card_id.as_uuid() == card)?;
    match crate::persona::card_holder::hold_of(found, now_ms) {
        crate::persona::card_holder::Hold::Held => found.owner.map(|o| o.as_uuid()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a failed grade that names no test, or no holder, or reads like a
    // score instead of an instruction (django-11749, 2026-09-07: the failing test sat in
    // the verdict file for two hours while the holder re-ran a repro that "looked good").
    #[test]
    fn a_failed_verdict_names_the_holder_and_the_failing_tests_and_hands_the_card_back() {
        let v = SweVerdict {
            instance_id: "django__django-11749".into(),
            resolved: false,
            f2p_passed: 0,
            f2p_total: 1,
            p2p_passed: 34,
            p2p_total: 34,
            gate_ok: true,
            failed_tests: vec!["test_mutually_exclusive_group_required_options".into()],
            ..Default::default()
        };
        let line = verdict_line(&v, Some("Joaquin"));
        assert!(line.starts_with("❌ @Joaquin django__django-11749 graded: not resolved"), "{line}");
        assert!(line.contains("still failing: test_mutually_exclusive_group_required_options"));
        assert!(line.contains("back with you as in-progress"));
        // No live holder: still a room line, never a panic, never a bare "@".
        let anon = verdict_line(&v, None);
        assert!(anon.starts_with("❌ django__django-11749"), "{anon}");
        let resolved = SweVerdict { resolved: true, f2p_passed: 1, ..v };
        assert!(verdict_line(&resolved, Some("Joaquin")).starts_with("✅ django__django-11749 RESOLVED"));
    }
}
