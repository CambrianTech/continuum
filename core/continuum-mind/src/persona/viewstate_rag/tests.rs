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
            continuum_positron::chat::test_roster_slot(
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
    let mut slot = continuum_positron::chat::test_roster_slot(
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
            continuum_positron::chat::test_roster_slot(
                Uuid::from_u128(1),
                "Paige",
                SenderKind::Agent,
            ),
            continuum_positron::chat::test_roster_slot(me, "Saoirse", SenderKind::Agent),
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
            continuum_positron::chat::test_roster_slot(
                Uuid::from_u128(1),
                "Paige",
                SenderKind::Agent,
            ),
            continuum_positron::chat::test_roster_slot(
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
    let delivery = source.deliver(&ctx()).await;
    let rendered = delivery
        .items
        .iter()
        .map(|i| i.content.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("Anwen"), "peer missing: {rendered}");
    assert!(rendered.contains("Asha"), "peer missing: {rendered}");
}

/// what this catches: a source that takes a size. Every member reaches the view as its
/// own item keyed by peer id (stable across turns, so an unchanged member appends
/// nothing); which level each shows at is the one allocation's choice, never the source's.
#[tokio::test]
async fn every_member_is_delivered_keyed_by_peer_id() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(roster_registry(&["Anwen", "Asha", "Anon"]));
    let delivery = source.deliver(&ctx()).await;
    assert_eq!(delivery.items.len(), 3, "no member is dropped: {:?}", delivery.items);
    assert!(!delivery.truncated);
    let keys: Vec<_> = delivery.items.iter().map(|i| i.key.clone()).collect();
    for id in 1..=3u128 {
        assert!(keys.contains(&Uuid::from_u128(id).to_string()), "{keys:?}");
    }
    assert!(
        delivery.items.iter().all(|i| i.pointer.is_some()),
        "a collapsed member stays reachable by name"
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
    let wake = crate::cognition::conversation::workspace_turns::build_workspace_turns(
        std::slice::from_ref(&delivery),
        "self",
        "Reader",
        None,
    );
    assert_eq!(wake.len(), 1);
    for name in admitted.values() {
        assert!(wake[0].content.contains(name));
    }
    let transcript = RagDelivery::of(
        "airc",
        (1..=3)
            .map(|i| {
                RagItem::whole(
                    format!("event-{i}"),
                    format!("distinct observation {i}"),
                    serde_json::json!({"peer_id": Uuid::from_u128(i).to_string()}),
                )
            })
            .collect(),
        false,
    );
    let own = Uuid::from_u128(3).to_string();
    let turns = crate::cognition::conversation::workspace_turns::build_workspace_turns(
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

/// what this catches: a source flagging `truncated` when it holds nothing more. The
/// flag now means only "more exists behind the expand command", never a size cut.
#[tokio::test]
async fn a_complete_delivery_is_not_truncated() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(roster_registry(&["Anwen"]));
    let delivery = source.deliver(&ctx()).await;
    assert_eq!(delivery.items.len(), 1);
    assert!(!delivery.truncated);
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
            roster: vec![continuum_positron::chat::test_roster_slot(
                Uuid::from_u128(42),
                "Saoirse",
                SenderKind::Agent,
            )],
        }));

    let source: ViewStateRagSource<RosterViewState> = ViewStateRagSource::per_room(rooms);
    let in_other =
        RagContext::for_persona_in_room(Uuid::from_u128(0x9), 0, Uuid::from_u128(OTHER));
    let delivery = source
        .deliver(&in_other)
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
            .deliver(&elsewhere)
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
    let delivery = source.deliver(&roomless).await;
    assert!(delivery.items.is_empty(), "{:?}", delivery.items);
    assert_eq!(delivery.tokens_used, 0);
    assert!(!delivery.truncated);
}

/// what this catches: a fabricated block. An empty room must render NOTHING —
/// a header with no members under it reads as "presence unknown" and is the
/// same lie as the framing that promised presence and delivered none.
#[tokio::test]
async fn an_empty_view_renders_no_block_rather_than_an_empty_header() {
    let source: ViewStateRagSource<RosterViewState> =
        ViewStateRagSource::per_room(Arc::new(PerRoomSubstrates::new()));
    let delivery = source.deliver(&ctx()).await;
    assert!(delivery.items.is_empty());
    assert_eq!(delivery.tokens_used, 0);
    assert!(!delivery.truncated);
}

