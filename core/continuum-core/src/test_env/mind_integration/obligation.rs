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
    let report = loop_one.report_interrupted(1).await.map(|found| found.report);
    assert_eq!(report, Some(InterruptedReport { open: 0, newly_interrupted: 0 }), "a fresh store owes nothing");
    let Admission::Confirmed(turn) = loop_one.admit(room, &trigger(room, "cut by the stop"), None, 2, None).await else {
        panic!("the turn is recorded and confirmed before her context is composed");
    };
    drop(loop_one);
    drop(before_stop);

    let first_restart = super::training_credit::data_runtime();
    let loop_two = Obligations::new(&first_restart, persona, name);
    assert_eq!(
        loop_two.report_interrupted(10).await.map(|found| found.report),
        Some(InterruptedReport { open: 1, newly_interrupted: 1 }),
        "the restart names the turn the stop cut"
    );
    drop(loop_two);
    drop(first_restart);

    let second_restart = super::training_credit::data_runtime();
    let loop_three = Obligations::new(&second_restart, persona, name);
    assert_eq!(
        loop_three.report_interrupted(20).await.map(|found| found.report),
        Some(InterruptedReport { open: 1, newly_interrupted: 0 }),
        "a second restart still owes it but does not count it again"
    );
    loop_three.close(Some(turn)).await;
    let conn = substrate_connection(&second_restart);
    assert!(open(&conn, name, persona).await.unwrap().is_empty(), "closing settles it"); // unwrap: the store under test
}

/// The store answers, then the reply is lost: `data/create` commits and the caller sees an
/// error. Every other call passes through.
struct LostCreateReply(crate::runtime::InProcessTransport, bool);

#[async_trait::async_trait]
impl continuum_client::Transport for LostCreateReply {
    async fn execute(&self, command: &str, params: serde_json::Value) -> Result<serde_json::Value, continuum_client::ClientError> {
        if self.1 && command == "data/list" {
            return Err(continuum_client::ClientError::Transport("read-back lost too".into()));
        }
        let answer = self.0.execute(command, params).await;
        if command == "data/create" {
            answer?;
            return Err(continuum_client::ClientError::Transport("reply lost after commit".into()));
        }
        answer
    }
    async fn subscribe(&self, class: &str) -> Result<continuum_client::event::EventStream, continuum_client::ClientError> {
        self.0.subscribe(class).await
    }
    async fn emit(&self, class: &str, payload: serde_json::Value) -> Result<(), continuum_client::ClientError> {
        self.0.emit(class, payload).await
    }
    async fn provide(&self, command: &str, handler: std::sync::Arc<dyn continuum_client::ServeHandler>) -> Result<(), continuum_client::ClientError> {
        self.0.provide(command, handler).await
    }
    async fn revoke(&self, command: &str) -> Result<(), continuum_client::ClientError> {
        self.0.revoke(command).await
    }
    async fn close(&self) -> Result<(), continuum_client::ClientError> {
        self.0.close().await
    }
}

// what this catches (Codex on S2): "the write failed" read as "no row". A create that
// committed but whose reply was lost must be reconciled to Confirmed by reading the row back,
// or its turn would be refused while a row a later start re-admits sits on disk.
#[tokio::test]
async fn a_committed_admission_whose_reply_was_lost_is_reconciled_to_confirmed() {
    let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global()).unwrap(); // unwrap: test fixture
    let executor = super::training_credit::data_runtime();
    let (name, persona, room) = ("obligation-lost-reply", Uuid::new_v4(), Uuid::new_v4());
    ensure_storage(&substrate_connection(&executor), name).await.unwrap(); // unwrap: the store under test
    let lossy = continuum_client::Connection::new(LostCreateReply(crate::runtime::InProcessTransport::new(executor.clone(), None), false));
    let row = PersonaObligation::admitted(persona, room, &trigger(room, "answered then lost"), None, 1);
    assert!(record(&lossy, name, &row).await.is_err(), "the caller sees the lost reply as a failure");
    assert_eq!(admit_reconciled(&lossy, name, &row).await, Admission::Confirmed(row.id), "the row is there: confirmed");
    assert_eq!(find(&substrate_connection(&executor), name, row.id).await.unwrap().map(|found| found.id), Some(row.id)); // unwrap: the store under test
}

// what this catches (stop-drain S2): an effect that reaches the world unrecorded. Each mark is
// written before its effect, the first sets effectAtMs and every one is kept in order (with the
// batch's call ids, so recovery can find receipts), and a mark on a row the store does not hold
// is refused without advancing the in-memory record.
#[tokio::test]
async fn effects_are_recorded_in_order_and_a_mark_without_a_row_is_refused() {
    use crate::cognition::act_observe::Effect;
    let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global()).unwrap(); // unwrap: test fixture
    let executor = super::training_credit::data_runtime();
    let (name, persona, room) = ("obligation-effects", Uuid::new_v4(), Uuid::new_v4());
    let obligations = Obligations::new(&executor, persona, name);
    assert!(obligations.report_interrupted(0).await.is_some(), "the store is readable");
    let Admission::Confirmed(turn) = obligations.admit(room, &trigger(room, "act then speak"), None, 1, None).await else {
        panic!("a healthy store confirms admission");
    };
    let gate = obligations.gate_for(turn);
    let batch = Effect::ToolBatch { calls: vec![("toolu_1".into(), "code/edit".into())] };
    gate.0.before(&batch).await.unwrap(); // unwrap: the mark under test
    gate.0.before(&Effect::Speech).await.unwrap(); // unwrap: the mark under test
    let row = find(&substrate_connection(&executor), name, turn).await.unwrap().unwrap(); // unwrap: the store under test
    assert!(row.effect_at_ms.is_some(), "the first effect is stamped");
    assert_eq!(
        row.effects,
        Some(vec![
            RecordedEffect::ToolBatch { calls: vec![("toolu_1".into(), "code/edit".into())] },
            RecordedEffect::Speech
        ]),
        "every effect, in order, with the batch's call ids"
    );

    let orphan = obligations.gate_for(Uuid::new_v4());
    assert!(orphan.0.before(&Effect::Speech).await.is_err(), "no row: refused, never permission");
}

// what this catches (Codex on #5492): a retry minting a SECOND row. The create commits, but its
// reply and the read-back are both lost, so the attempt is Unconfirmed(A). The retry must
// reconcile A itself: confirm A, leave exactly one row, never mint B beside an open A that a
// later start would replay as untouched.
#[tokio::test]
async fn a_retry_reconciles_the_same_candidate_and_never_mints_a_second_row() {
    let home = tempfile::tempdir().unwrap(); // unwrap: test fixture
    let _home = HomeGuard::set(home.path()).await;
    crate::persona::register_substrate_orm_entities(crate::orm::OrmEntityRegistry::global()).unwrap(); // unwrap: test fixture
    let executor = super::training_credit::data_runtime();
    let (name, persona, room) = ("obligation-candidate", Uuid::new_v4(), Uuid::new_v4());
    let healthy = substrate_connection(&executor);
    ensure_storage(&healthy, name).await.unwrap(); // unwrap: the store under test
    let blind = continuum_client::Connection::new(LostCreateReply(crate::runtime::InProcessTransport::new(executor.clone(), None), true));
    let row = PersonaObligation::admitted(persona, room, &trigger(room, "committed unseen"), None, 1);
    let Admission::Unconfirmed(candidate) = admit_reconciled(&blind, name, &row).await else {
        panic!("no reply and no read-back: unconfirmed");
    };
    assert_eq!(candidate, row.id, "the attempt names its candidate");

    let obligations = Obligations::new(&executor, persona, name);
    assert_eq!(
        obligations.admit(room, &trigger(room, "committed unseen"), None, 2, Some(candidate)).await,
        Admission::Confirmed(candidate),
        "the retry confirms the same row"
    );
    let rows = open(&healthy, name, persona).await.unwrap(); // unwrap: the store under test
    assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), vec![candidate], "one row, never a second");
}
