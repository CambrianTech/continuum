use super::*;
use crate::persona::roster_hold::RosterHold;

fn home(name: &str) -> Home {
    Home { id: PeerId::from_uuid(uuid::Uuid::new_v4()), agent_name: name.to_string() }
}

fn hold(names: &[&str]) -> RosterHold {
    RosterHold {
        only: names.iter().map(|s| s.to_string()).collect(),
        until_ms: u64::MAX,
        reason: "M5 population".into(),
        exclusive: true,
    }
}

async fn ledger_at(path: &std::path::Path) -> DormancyLedger {
    let adapter = crate::orm::SqliteAdapter::open(path).await.expect("adapter"); // test: a failure here is the assertion failing loudly
    DormancyLedger::open(Arc::new(adapter)).await.expect("ledger") // test: a failure here is the assertion failing loudly
}

fn names(rows: &[PersonaWakefulness]) -> Vec<&str> {
    let mut v: Vec<&str> = rows.iter().map(|r| r.agent_name.as_str()).collect();
    v.sort_unstable();
    v
}

// what this catches: the M5's twelve coming back when the hold lapses. The first boot
// with durable dormancy writes every home not on the hold and not online as Dormant, the
// held and the online stay Awake, and once the table has rows a later boot (hold gone)
// keeps the same minds down instead of migrating again.
#[tokio::test]
async fn the_migration_marks_unheld_homes_dormant_and_outlives_the_hold() {
    let dir = tempfile::tempdir().expect("tempdir"); // test: a failure here is the assertion failing loudly
    let path = dir.path().join("wakefulness.sqlite");
    let homes = vec![home("Aiko"), home("Aris"), home("Idris"), home("Cyrus")];
    let h = hold(&["aiko", "Aris"]);
    let online = vec!["Cyrus".to_string()];
    let dormant = ledger_at(&path)
        .await
        .load(&homes, Migration { hold: Some(&h), online: &online, now_ms: 7 })
        .await
        .expect("load"); // test: a failure here is the assertion failing loudly
    assert_eq!(names(&dormant), vec!["Idris"], "only the home neither held nor online sleeps");
    match &dormant[0].state {
        Wakefulness::Dormant { reason, since_ms, .. } => {
            assert_eq!(reason, MIGRATION_REASON);
            assert_eq!(*since_ms, 7);
        }
        other => panic!("migrated as {other:?}"),
    }
    let after_expiry = ledger_at(&path)
        .await
        .load(&homes, Migration { hold: None, online: &[], now_ms: 9 })
        .await
        .expect("reload"); // test: a failure here is the assertion failing loudly
    assert_eq!(names(&after_expiry), vec!["Idris"], "the hold lapsed; she stays down");
    // Without an exclusive hold the migration keeps everyone awake.
    assert!(migrate_from_hold(&homes, Migration { hold: None, online: &[], now_ms: 1 }).iter().all(|r| !r.is_dormant()));
}

// what this catches: the population decision lost to deploy timing (Cormac on #5509): a
// first boot AFTER the hold lapsed still migrates its exclusions. An expired four-name
// hold over ten homes writes six Dormant and four Awake.
#[tokio::test]
async fn a_boot_after_the_hold_lapsed_still_migrates_it() {
    let dir = tempfile::tempdir().expect("tempdir"); // test: a failure here is the assertion failing loudly
    let hold_path = dir.path().join("roster-hold.json");
    let mut lapsed = hold(&["Aiko", "Aris", "Atlas", "Benchy"]);
    lapsed.until_ms = 5;
    std::fs::write(&hold_path, serde_json::to_vec(&lapsed).expect("ser")).expect("write"); // test: a failure here is the assertion failing loudly
    assert!(crate::persona::roster_hold::active_at(&hold_path, 10).is_none(), "the hold has lapsed");
    let recorded = crate::persona::roster_hold::last_recorded_at(&hold_path).expect("kept after the lapse"); // test: a failure here is the assertion failing loudly
    let homes: Vec<Home> = ["Aiko", "Aris", "Atlas", "Benchy", "Idris", "Joaquin", "Cyrus", "Esme", "Freya", "Demetri"]
        .iter()
        .map(|n| home(n))
        .collect();
    let dormant = ledger_at(&dir.path().join("wakefulness.sqlite"))
        .await
        .load(&homes, Migration { hold: Some(&recorded), online: &[], now_ms: 10 })
        .await
        .expect("load"); // test: a failure here is the assertion failing loudly
    assert_eq!(names(&dormant), vec!["Cyrus", "Demetri", "Esme", "Freya", "Idris", "Joaquin"], "six dormant, the four held awake");
}

// what this catches: a despawn-to-dormant that dissolves on the next restart (the
// 2026-08-23 shape: despawn was "not now", nothing recorded "not until I say"). Her row
// is read back by a NEW ledger over the same store; a row for a home no longer on disk
// is not held (it would take a seat off the count for a mind that cannot be drawn).
#[tokio::test]
async fn a_dormant_mind_stays_dormant_across_a_restart() {
    let dir = tempfile::tempdir().expect("tempdir"); // test: a failure here is the assertion failing loudly
    let path = dir.path().join("wakefulness.sqlite");
    let (freya, gone) = (home("Freya"), home("Gone"));
    let homes = vec![freya.clone(), home("Benchy")];
    {
        let ledger = ledger_at(&path).await;
        assert!(ledger.load(&homes, Migration { hold: None, online: &[], now_ms: 1 }).await.expect("load").is_empty()); // test: a failure here is the assertion failing loudly
        let dormant = Wakefulness::Dormant { since_ms: 2, reason: "population".into(), by: "owner".into() };
        ledger.set(freya.id, &freya.agent_name, dormant.clone(), 2).await.expect("sleep"); // test: a failure here is the assertion failing loudly
        ledger.set(gone.id, &gone.agent_name, dormant, 2).await.expect("sleep"); // test: a failure here is the assertion failing loudly
    }
    let restarted = ledger_at(&path).await;
    let held = restarted.load(&homes, Migration { hold: None, online: &[], now_ms: 3 }).await.expect("reload"); // test: a failure here is the assertion failing loudly
    assert_eq!(names(&held), vec!["Freya"], "she is still dormant; a row without a home is not held");
    assert_eq!(held[0].id, freya.id, "the row is hers, by her peer id");
}

// what this catches: a wake that is lost (or undone) by the next restart, and an older
// write landing over a newer one. Woken, she reads Awake from a fresh ledger and the draw
// seats her again.
#[tokio::test]
async fn wake_resumes_her_and_the_wake_is_durable() {
    use crate::persona::spawner_module::OperatorRoster;
    let dir = tempfile::tempdir().expect("tempdir"); // test: a failure here is the assertion failing loudly
    let path = dir.path().join("wakefulness.sqlite");
    let esme = home("Esme");
    let homes = vec![esme.clone()];
    let ledger = ledger_at(&path).await;
    ledger
        .set(esme.id, "Esme", Wakefulness::Dormant { since_ms: 1, reason: "population".into(), by: "owner".into() }, 1)
        .await
        .expect("sleep"); // test: a failure here is the assertion failing loudly
    let dormant = ledger.load(&homes, Migration { hold: None, online: &[], now_ms: 2 }).await.expect("load"); // test: a failure here is the assertion failing loudly
    assert_eq!(names(&dormant), vec!["Esme"]);
    ledger.set(esme.id, "Esme", Wakefulness::Awake { since_ms: 3, by: "owner".into() }, 3).await.expect("wake"); // test: a failure here is the assertion failing loudly
    ledger
        .set(esme.id, "Esme", Wakefulness::Dormant { since_ms: 0, reason: "stale".into(), by: "late".into() }, 0)
        .await
        .expect("an older write is accepted and ignored"); // test: a failure here is the assertion failing loudly
    let restarted = ledger_at(&path).await;
    let dormant = restarted.load(&homes, Migration { hold: None, online: &[], now_ms: 4 }).await.expect("reload"); // test: a failure here is the assertion failing loudly
    assert!(dormant.is_empty(), "awake after the restart: {dormant:?}");
    let roster = OperatorRoster { hold: None, resting: Vec::new(), dormant: DormantSet::Known(dormant) };
    let plan = vec![crate::persona::spawner_module::DesiredRole {
        role: crate::experience::role::RoleId::Helper,
        model_id: "m".into(),
        lanes: 1,
        served_context_window: 4096,
    }];
    let mut provider = OneHome(Some(esme));
    let drawn = crate::persona::spawner_module::draw_intents(&mut provider, &plan, &roster, 0).await.expect("draw"); // test: a failure here is the assertion failing loudly
    assert_eq!(drawn.iter().map(|i| i.agent_name.as_str()).collect::<Vec<_>>(), vec!["Esme"], "the draw seats her again");
}

struct OneHome(Option<Home>);

#[async_trait::async_trait]
impl crate::persona::identity_provider::PersonaIdentityProvider for OneHome {
    fn name(&self) -> &'static str {
        "one-home"
    }
    async fn next_persona(
        &mut self,
    ) -> Result<Option<crate::persona::identity_provider::PersonaIdentityIntent>, crate::persona::identity_provider::PersonaIdentityError>
    {
        Ok(self.0.take().map(|h| crate::persona::identity_provider::PersonaIdentityIntent {
            persona_id: h.id.as_uuid(),
            agent_name: h.agent_name,
            source: crate::persona::identity_provider::PersonaIdentitySource::ResumedFromDisk,
        }))
    }
}
