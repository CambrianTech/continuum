#![cfg(test)]
//! From `persona::obligation` (card 920619c5): a persona's admitted-turn rows through core's
//! REAL `DataModule` over SQLite, under the production `GridTrustAuthPolicy`, on the
//! substrate connection the service loop uses.

use uuid::Uuid;

use crate::cognition::conversation::IncomingMessage;
use crate::paths::test_home::HomeGuard;
use crate::persona::obligation::*;
use crate::persona::training_producer::substrate_connection;

fn trigger(room: Uuid, text: &str) -> IncomingMessage {
    IncomingMessage {
        event_id: Uuid::new_v4(),
        lamport: 3,
        peer_id: Uuid::new_v4(),
        text: text.into(),
        media: Vec::new(),
        room_id: room,
        work: None,
    }
}

// what this catches: an interrupted turn the next start cannot see. The row must outlive the
// process that wrote it (a real store), read back per persona and oldest first, be named once
// across two starts (the stamp), and vanish when the turn closes.
#[tokio::test]
async fn admitted_turns_persist_until_closed_and_each_start_names_only_what_it_interrupted() {
    let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global()).unwrap(); // unwrap: test fixture
    let executor = super::training_credit::data_runtime();
    let conn = substrate_connection(&executor);
    let (name, persona, other) = ("obligation-ledger", Uuid::new_v4(), Uuid::new_v4());
    let room = Uuid::new_v4();
    ensure_storage(&conn, name).await.unwrap(); // unwrap: the store under test

    let first = PersonaObligation::admitted(persona, room, &trigger(room, "first"), Some(Uuid::new_v4()), 10);
    let second = PersonaObligation::admitted(persona, room, &trigger(room, "second"), None, 20);
    let theirs = PersonaObligation::admitted(other, room, &trigger(room, "not hers"), None, 5);
    for row in [&second, &first, &theirs] {
        record(&conn, name, row).await.unwrap(); // unwrap: the store under test
    }

    let open_rows = open(&conn, name, persona).await.unwrap(); // unwrap: the store under test
    assert_eq!(open_rows, vec![first.clone(), second.clone()], "hers only, oldest first, whole");

    // The first start stamps both; a second start finds nothing new.
    for row in newly_interrupted(&open_rows) {
        mark_interrupted(&conn, name, row.id, 30).await.unwrap(); // unwrap: the store under test
    }
    let after_first_start = open(&conn, name, persona).await.unwrap(); // unwrap: the store under test
    assert_eq!(newly_interrupted(&after_first_start).count(), 0, "a second start names nothing new");
    assert!(after_first_start.iter().all(|row| row.interrupted_at_ms == Some(30)));

    close(&conn, name, first.id).await.unwrap(); // unwrap: the store under test
    close(&conn, name, second.id).await.unwrap(); // unwrap: the store under test
    assert!(open(&conn, name, persona).await.unwrap().is_empty(), "a closed turn is owed nothing"); // unwrap: the store under test
    assert_eq!(open(&conn, name, other).await.unwrap(), vec![theirs]); // unwrap: the store under test
}

// what this catches (Codex on #5480): an obligation that lives only as long as the process that
// wrote it. A turn is admitted through one executor (one core), then each "restart" is a fresh
// executor over the same SQLite home, as a new core process would be. The first restart names
// the interrupted turn, the second names nothing new but still owes it, and closing settles it.
#[tokio::test]
async fn an_admitted_turn_survives_restarts_and_each_restart_reports_it_once() {
    let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global()).unwrap(); // unwrap: test fixture
    let (name, persona, room) = ("obligation-restart", Uuid::new_v4(), Uuid::new_v4());

    let before_stop = super::training_credit::data_runtime();
    let loop_one = Obligations::new(&before_stop, persona, name);
    let report = loop_one.report_interrupted(1).await;
    assert_eq!(report, Some(InterruptedReport { open: 0, newly_interrupted: 0 }), "a fresh store owes nothing");
    let turn = loop_one.admit(room, &trigger(room, "cut by the stop"), None, 2).await;
    assert!(turn.is_some(), "the turn is recorded before her context is composed");
    drop(loop_one);
    drop(before_stop);

    let first_restart = super::training_credit::data_runtime();
    let loop_two = Obligations::new(&first_restart, persona, name);
    assert_eq!(
        loop_two.report_interrupted(10).await,
        Some(InterruptedReport { open: 1, newly_interrupted: 1 }),
        "the restart names the turn the stop cut"
    );
    drop(loop_two);
    drop(first_restart);

    let second_restart = super::training_credit::data_runtime();
    let loop_three = Obligations::new(&second_restart, persona, name);
    assert_eq!(
        loop_three.report_interrupted(20).await,
        Some(InterruptedReport { open: 1, newly_interrupted: 0 }),
        "a second restart still owes it but does not count it again"
    );
    loop_three.close(turn).await;
    let conn = substrate_connection(&second_restart);
    assert!(open(&conn, name, persona).await.unwrap().is_empty(), "closing settles it"); // unwrap: the store under test
}
