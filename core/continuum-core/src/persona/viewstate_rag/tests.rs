#![cfg(test)]

use super::*;
use continuum_positron::{RosterViewState, SenderKind, StateBuilder};
use uuid::Uuid;

use continuum_positron::scoping::PerRoomSubstrates;
use std::sync::Arc;

/// The room every roster fixture below is stamped with, and the one [`ctx`]
/// puts the turn in.
const ROOM: u128 = 0xaa;

/// Store a roster for `room` in that room's OWN substrate and hand back the
/// registry — the shape production uses. Tests go through the registry rather
/// than a bare `Substrate` so they exercise the live binding: a roster test
/// that constructs a single substrate cannot fail when the source binds the
/// wrong room, which is exactly how #3862 stayed green.
fn roster_registry_for(room: u128, names: &[&str]) -> Arc<PerRoomSubstrates> {
    let rooms = Arc::new(PerRoomSubstrates::new());
    let builder = StateBuilder::standalone();
    let roster = names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(i as u128 + 1),
                name,
                SenderKind::Agent,
            )
        })
        .collect();
    rooms
        .for_room(Uuid::from_u128(room))
        .store(builder.session(RosterViewState {
            room_id: Uuid::from_u128(room),
            roster,
        }));
    rooms
}

fn roster_registry(names: &[&str]) -> Arc<PerRoomSubstrates> {
    roster_registry_for(ROOM, names)
}

/// A turn context stamped with the SAME room the fixtures build, so the
/// shared room gate allows delivery (an unstamped ctx would pass too, but
/// stamping is what a live turn does).
fn ctx() -> RagContext {
    RagContext::for_persona_in_room(Uuid::from_u128(0x9), 0, Uuid::from_u128(ROOM))
}

// what this catches: the mind's roster line carries NO liveness flag — `[ready]`/
// `[busy]` flips per heartbeat inside the STABLE system prefix and forfeited the
// whole prefix (hit_rate 0.0 on 57/57 turns, prompts diverging at the flag,
// 2026-09-04). Name, kind and role stay; presence is the live view's job.
#[test]
fn the_roster_line_carries_no_liveness_flag() {
    let mut slot = crate::ipc::positron_source::test_roster_slot(
        Uuid::from_u128(7),
        "Anwen",
        SenderKind::Agent,
    );
    slot.availability = Some("busy".to_string());
    let view = RosterViewState {
        room_id: Uuid::from_u128(0xaa),
        roster: vec![slot],
    };
    let units = RagRenderable::units(&view, Uuid::from_u128(7));
    assert_eq!(units.len(), 1);
    assert!(units[0].content.starts_with("Anwen"), "{units:?}");
    assert!(
        !units[0].content.contains("[busy]") && !units[0].content.contains('['),
        "{units:?}"
    );
}

/// what this catches (card bb387821): the citizen's OWN row rendering
/// indistinguishably from her peers'. Measured on IntelMac in BOTH directions with
/// correct prompts — Paige, told "You are Paige" five times plus an explicit
/// NOT-list, answered "I am Saoirse"; Saoirse answered "My name is Paige". The
/// structured list beat the prose. Asserts the marker lands on exactly the reader's
/// row and on no other, and that she is still LISTED (removing her would undercount
/// who is in the room).
#[test]
fn a_citizens_own_row_is_marked_and_only_hers() {
    let me = Uuid::from_u128(2);
    let view = RosterViewState {
        room_id: Uuid::from_u128(0xaa),
        roster: vec![
            crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(1),
                "Paige",
                SenderKind::Agent,
            ),
            crate::ipc::positron_source::test_roster_slot(me, "Saoirse", SenderKind::Agent),
        ],
    };
    let units = RagRenderable::units(&view, me);
    assert_eq!(units.len(), 2, "she stays listed: {units:?}");
    let mine: Vec<_> = units.iter().filter(|u| u.content.contains("YOU")).collect();
    assert_eq!(mine.len(), 1, "exactly one row marked: {units:?}");
    assert!(
        mine[0].content.starts_with("Saoirse"),
        "the READER's row: {units:?}"
    );
    assert!(
        units
            .iter()
            .any(|u| u.content.starts_with("Paige") && !u.content.contains("YOU")),
        "the peer stays unmarked: {units:?}"
    );
}

/// what this catches: marking by position or by accident rather than by identity.
/// A viewer who is not in this roster at all — a human reading a room she has not
/// joined, or a stale persona_id — must see NO marker, not the first row marked.
#[test]
fn a_viewer_absent_from_the_roster_marks_nobody() {
    let view = RosterViewState {
        room_id: Uuid::from_u128(0xaa),
        roster: vec![
            crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(1),
                "Paige",
                SenderKind::Agent,
            ),
            crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(2),
                "Saoirse",
                SenderKind::Agent,
            ),
        ],
    };
    let units = RagRenderable::units(&view, Uuid::from_u128(0xdead));
    assert_eq!(units.len(), 2);
    assert!(
        !units.iter().any(|u| u.content.contains("YOU")),
        "{units:?}"
    );
}

/// what this catches: THE defect. A peer present in the room must reach the
/// citizen's grounding — measured live at ZERO occurrences while the browser
/// rendered the same peer fine, because the two read different code. If this
/// regresses, citizens go blind to each other again and nothing else fails.
#[tokio::test]
async fn a_present_peer_reaches_the_citizens_grounding() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(roster_registry(&["Anwen", "Asha"]));
    // The canonical kind owns the same small claim as its legacy reader;
    // adapting it through ViewState must not buy a retrieval-sized share.
    for window in [0, 256, 2048, 51_712] {
        let claim = source.budget_claim(window, window);
        assert_eq!(claim.max_tokens, window / 64);
        assert_eq!(claim.min_tokens, 0);
        assert_eq!(claim.floor_tokens, 0);
    }
    let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
    let rendered = delivery
        .items
        .iter()
        .map(|i| i.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Anwen"), "peer missing: {rendered}");
    assert!(rendered.contains("Asha"), "peer missing: {rendered}");
    assert!(delivery.tokens_used <= 500);
}

/// what this catches: the all-or-nothing failure `floor_tokens` exists to
/// prevent. Under a budget that fits one member, the citizen must learn about
/// ONE person — not receive an empty block, and not overrun the window.
#[tokio::test]
async fn a_tight_budget_yields_fewer_units_never_a_chopped_one() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(roster_registry(&["Anwen", "Asha", "Anon"]));
    // 8 tokens, against ~4-token member lines: fits two, not three. (12 was
    // the first guess and it fit ALL THREE exactly — the packing was right and
    // the test's arithmetic was wrong, which is worth saying out loud because
    // "make the failing assert pass" is how a real invariant gets weakened.)
    let delivery = source.deliver(&ctx(), 8, ResolutionPreference::Raw).await;
    assert!(!delivery.items.is_empty(), "a fitting unit must be delivered");
    assert!(delivery.items.len() < 3, "budget 8 cannot fit all three");
    assert!(
        delivery.tokens_used <= 8,
        "the allocator's contract is tokens_used <= budget, got {}",
        delivery.tokens_used
    );
    assert!(
        delivery.continuation.is_some(),
        "undelivered members must be resumable, not silently dropped"
    );
    // Regression for card 84dee23e: live roster atoms must feed the actual
    // turn-author and wake consumers, including under partial admission.
    // A source-id alias alone cannot pass: the producer must retain identity.
    let admitted: std::collections::HashMap<_, _> = delivery
        .items
        .iter()
        .map(|item| {
            (
                item.metadata["peer_id"].as_str().unwrap().to_owned(),
                item.metadata["display_name"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    assert_eq!(admitted.len(), delivery.items.len());
    for (id, name) in [(1, "Anwen"), (2, "Asha"), (3, "Anon")] {
        if delivery.items.iter().any(|item| item.content.starts_with(name)) {
            assert_eq!(admitted.get(&Uuid::from_u128(id).to_string()).map(String::as_str), Some(name));
        }
    }
    let wake = super::super::service_loop::build_workspace_turns(
        std::slice::from_ref(&delivery),
        "self",
        "Reader",
        None,
    );
    assert_eq!(wake.len(), 1);
    for name in admitted.values() {
        assert!(wake[0].content.contains(name));
    }
    let transcript = RagDelivery {
        source_id: "airc".into(),
        items: (1..=3)
            .map(|i| RagItem {
                content: format!("distinct observation {i}"),
                tokens: 4,
                metadata: serde_json::json!({"peer_id": Uuid::from_u128(i).to_string()}),
            })
            .collect(),
        tokens_used: 12,
        continuation: None,
        resolution_used: ResolutionPreference::Raw,
    };
    let own = Uuid::from_u128(3).to_string();
    let turns = super::super::service_loop::build_workspace_turns(
        &[delivery, transcript],
        &own,
        "Native identity",
        None,
    );
    assert!(turns.len() >= 3, "all transcript events survive attribution");
    for (index, turn) in turns.iter().take(3).enumerate() {
        let peer = Uuid::from_u128(index as u128 + 1).to_string();
        assert_eq!(turn.is_self, peer == own);
        let expected = if peer == own {
            "Native identity"
        } else {
            admitted.get(&peer).map(String::as_str).unwrap_or(&peer)
        };
        assert_eq!(turn.author, expected);
    }
}

/// what this catches: an unconditional cursor. If a fully-delivered source
/// still handed back a cursor, the allocator would spend a later turn's budget
/// resuming a source with nothing left to say.
#[tokio::test]
async fn a_complete_delivery_offers_no_continuation() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(roster_registry(&["Anwen"]));
    let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
    assert_eq!(delivery.items.len(), 1);
    assert!(delivery.continuation.is_none());
}

/// what this catches (regression for #3862): a room-scoped source that binds ONE
/// room and answers with it forever. The roster was constructed as
/// `for_room(identity.default_room)`, so a citizen working anywhere else got her
/// default room's view, the shared room gate correctly refused it, and she stood
/// in a room full of people seeing nobody — 662 abstains in 873 ticks, bound
/// #general, every turn in #continuum.
///
/// The registry here holds BOTH rooms, which is what production looks like:
/// `positron_source::sink` writes every room's view unconditionally. So this
/// fails on the old binding not because the other room's data is missing, but
/// because the source never asked for it.
#[tokio::test]
async fn the_roster_follows_the_turn_into_a_room_that_is_not_the_first_one() {
    const OTHER: u128 = 0xbb;
    let rooms = roster_registry(&["Anwen"]);
    // The SECOND room's own roster, stored beside the first — different people.
    let builder = StateBuilder::standalone();
    rooms
        .for_room(Uuid::from_u128(OTHER))
        .store(builder.session(RosterViewState {
            room_id: Uuid::from_u128(OTHER),
            roster: vec![crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(42),
                "Saoirse",
                SenderKind::Agent,
            )],
        }));

    let source: ViewStateRagSource<RosterViewState> = ViewStateRagSource::per_room(rooms);
    let in_other =
        RagContext::for_persona_in_room(Uuid::from_u128(0x9), 0, Uuid::from_u128(OTHER));
    let delivery = source
        .deliver(&in_other, 500, ResolutionPreference::Raw)
        .await;
    let rendered = delivery
        .items
        .iter()
        .map(|i| i.content.clone())
        .collect::<Vec<_>>()
        .join(
            "
",
        );
    assert!(
        rendered.contains("Saoirse"),
        "a turn in a room must be grounded in THAT room's people: {rendered}"
    );
    assert!(
        !rendered.contains("Anwen"),
        "and never in another room's: {rendered}"
    );
}

/// what this catches: a continuation resumed in the WRONG ROOM. A cursor is an
/// offset into one room's unit list. Before the per-room binding this was safe
/// by accident — the source read the same room forever, so every cursor
/// indexed the same list. Per-room resolution makes an A-issued cursor
/// resumable in B, where the offset would silently skip B's first N people.
///
/// The `room_scope_allows` call in `deliver_continuation` does NOT catch it:
/// per-room resolution makes `view.room()` equal the turn's room by
/// construction, so that gate is now vacuous there. This is the guard that
/// replaced its teeth.
///
/// FIXTURE NOTE, and it is the point: the other room must hold MORE members
/// than the cursor's offset. The first version of this test gave it one, and
/// it passed with the guard disabled — `from >= total` returned `None` for the
/// wrong reason, so the test could not tell a working guard from a missing
/// one. Caught by mutation-testing it. A fixture with fewer degrees of freedom
/// than the bug cannot fail, which is the same lesson as the single-`Substrate`
/// roster tests this PR replaced.
#[tokio::test]
async fn a_continuation_minted_in_one_room_is_refused_in_another() {
    const OTHER: u128 = 0xbb;
    let rooms = roster_registry(&["Anwen", "Asha", "Anon"]);
    let builder = StateBuilder::standalone();
    let other_roster: Vec<_> = ["Saoirse", "Solenne", "Sorcha", "Sinead", "Siofra"]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(40 + i as u128),
                name,
                SenderKind::Agent,
            )
        })
        .collect();
    let other_len = other_roster.len();
    rooms
        .for_room(Uuid::from_u128(OTHER))
        .store(builder.session(RosterViewState {
            room_id: Uuid::from_u128(OTHER),
            roster: other_roster,
        }));
    let source: ViewStateRagSource<RosterViewState> = ViewStateRagSource::per_room(rooms);

    // A tight budget in ROOM yields a cursor with units left over.
    let first = source.deliver(&ctx(), 8, ResolutionPreference::Raw).await;
    let cursor = first
        .continuation
        .expect("a partial delivery must offer a cursor");

    // The offset MUST land inside the other room's list, or the `from >= total`
    // check refuses for an unrelated reason and this test proves nothing.
    let offset = cursor.opaque["next"].as_u64().expect("cursor carries next") as usize;
    assert!(
        offset < other_len,
        "fixture is degenerate: offset {offset} is past the other room's              {other_len} members, so a MISSING guard would also return None"
    );

    // Same persona, same source, DIFFERENT room: must refuse rather than index
    // into the other room's roster.
    let in_other =
        RagContext::for_persona_in_room(Uuid::from_u128(0x9), 0, Uuid::from_u128(OTHER));
    assert!(
        source
            .deliver_continuation(&in_other, cursor.clone(), 500)
            .await
            .is_none(),
        "a cursor from another room must not resume here"
    );

    // ...and the SAME room still resumes, so the guard is not just refusing
    // everything (a guard that never lets anything through is not a guard).
    let resumed = source
        .deliver_continuation(&ctx(), cursor, 500)
        .await
        .expect("same-room resume must still work");
    assert!(!resumed.items.is_empty());
}

/// what this catches (bc66becf; sibling of db0aa5b2 on the wall): a continuation
/// resumed positionally into a list that CHURNED between mint and resume. The
/// offset `next` indexes whatever unit list is re-computed at resume time — if a
/// peer left (or a row dropped out) before it, every later unit shifts left by one
/// and the same offset skips exactly one person who is still in the room. The
/// cursor therefore carries a fingerprint of the list it indexes into
/// (`list_fingerprint`), and resuming into a churned list is refused: dropping one
/// continuation costs a re-read from the top — bounded, self-healing, never skips.
///
/// Fixture note, same lesson as the cross-room test above: for a positional resume
/// to skip someone, the removal must sit at or before the offset. The list is
/// name-ordered (units sorts by display_name), so the peer dropped here — Anwen —
/// sits at index 1, ahead of this fixture's offset (k=2 under an 8-token budget):
/// pre-fix the same index resumed past Aoife and she was never delivered; now it
/// must refuse.
#[tokio::test]
async fn a_continuation_is_refused_when_the_roster_churns_before_its_offset() {
    let rooms = roster_registry(&["Anwen", "Asha", "Anon", "Aoife"]);
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(Arc::clone(&rooms));

    // A tight budget yields a cursor with units left over; the offset is read from
    // it, so the test holds for whatever `pack` decides fits under 8 tokens.
    let first = source.deliver(&ctx(), 8, ResolutionPreference::Raw).await;
    let cursor = first.continuation.expect("a partial delivery must offer a cursor");

    // Churn: one peer leaves before the offset. Re-storing the room's view without
    // her is exactly what `positron_source::sink` does on every roster update.
    let builder = StateBuilder::standalone();
    rooms.for_room(Uuid::from_u128(ROOM)).store(builder.session(RosterViewState {
        room_id: Uuid::from_u128(ROOM),
        roster: vec![
            crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(2),
                "Asha",
                SenderKind::Agent,
            ),
            crate::ipc::positron_source::test_roster_slot(Uuid::from_u128(3), "Anon", SenderKind::Agent),
            crate::ipc::positron_source::test_roster_slot(
                Uuid::from_u128(4),
                "Aoife",
                SenderKind::Agent,
            ),
        ],
    }));

    // Pre-fix this resumed from the same index into [Asha, Anon, Aoife] and
    // silently dropped whoever sat at that position; now it must refuse.
    assert!(
        source.deliver_continuation(&ctx(), cursor, 500).await.is_none(),
        "a churned list must not be indexed positionally — the offset would skip one"
    );
}

/// what this catches: the other direction of the fingerprint guard. A continuation
/// resumed into an UNCHANGED list must land exactly where it left off — everyone at
/// or after the offset present, nobody before it re-appearing (resuming from zero
/// would re-ground peers the first delivery already spent). A guard that also
/// refused stable resumes would cost a re-read every turn and starve long rosters.
#[tokio::test]
async fn a_continuation_resumes_exactly_where_it_left_off_when_the_list_is_stable() {
    let names = ["Anwen", "Asha", "Anon", "Aoife"];
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(roster_registry(&names));

    let first = source.deliver(&ctx(), 8, ResolutionPreference::Raw).await;
    let cursor = first.continuation.expect("a partial delivery must offer a cursor");
    let k = cursor.opaque["next"].as_u64().expect("cursor carries next") as usize;

    let resumed = source
        .deliver_continuation(&ctx(), cursor, 500)
        .await
        .expect("an unchanged list must still resume");
    let joined: String = resumed.items.iter().map(|i| i.content.clone()).collect::<Vec<_>>().join("\n");

    // `units` renders name-ordered — it sorts by display_name so the prompt's line
    // order stays stable for KV reuse — so the expectation must come from THAT
    // order, not fixture insertion order (which put Anon after this offset and
    // failed CI; the resumed tail was correctly Aoife, Asha).
    let mut ordered = names.to_vec();
    ordered.sort_unstable();
    for name in &ordered[k..] {
        assert!(joined.contains(name), "a peer at or after the offset must resume: {joined}");
    }
    for name in &ordered[..k] {
        assert!(!joined.contains(name), "a peer before the offset must not re-appear: {joined}");
    }
}

/// what this catches (review of #3879): an INSERTING read. `for_room` creates
/// the entry it is asked about; this source now looks up a room on EVERY
/// delivery, so using it here would let a citizen wandering rooms accrete a
/// permanently-empty substrate per distinct room — invisible until a node has
/// been up a week, and it would inflate `room_count`, an ops read, with rooms
/// nothing ever projected. Asserts the read path leaves the map's size alone.
#[tokio::test]
async fn reading_a_room_that_was_never_projected_does_not_create_one() {
    let rooms = roster_registry(&["Anwen"]);
    assert_eq!(rooms.room_count(), 1, "fixture projects exactly one room");
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(Arc::clone(&rooms));
    for n in 0..5u128 {
        let elsewhere =
            RagContext::for_persona_in_room(Uuid::from_u128(0x9), 0, Uuid::from_u128(0xd0 + n));
        let delivery = source
            .deliver(&elsewhere, 500, ResolutionPreference::Raw)
            .await;
        assert!(delivery.items.is_empty(), "an unprojected room has no view");
    }
    assert_eq!(
        rooms.room_count(),
        1,
        "five deliveries into unprojected rooms must not create five substrates"
    );
}

/// what this catches: the honest absence on a roomless turn. Background
/// consolidation and idle ticks carry no room, and a room-scoped source has no
/// room whose people it could report. It must render nothing rather than reach
/// for whatever room happens to be in the registry — the fallback that would
/// silently reintroduce exactly the cross-room bleed above.
#[tokio::test]
async fn a_roomless_turn_gets_no_roster_rather_than_an_arbitrary_rooms() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(roster_registry(&["Anwen"]));
    let roomless = RagContext::for_persona(Uuid::from_u128(0x9), 0);
    let delivery = source.deliver(&roomless, 500, ResolutionPreference::Raw).await;
    assert!(delivery.items.is_empty(), "{:?}", delivery.items);
    assert_eq!(delivery.tokens_used, 0);
    assert!(delivery.continuation.is_none());
}

/// what this catches: a fabricated block. An empty room must render NOTHING —
/// a header with no members under it reads as "presence unknown" and is the
/// same lie as the framing that promised presence and delivered none.
#[tokio::test]
async fn an_empty_view_renders_no_block_rather_than_an_empty_header() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(Arc::new(PerRoomSubstrates::new()));
    let delivery = source.deliver(&ctx(), 500, ResolutionPreference::Raw).await;
    assert!(delivery.items.is_empty());
    assert_eq!(delivery.tokens_used, 0);
    assert!(delivery.continuation.is_none());
}

