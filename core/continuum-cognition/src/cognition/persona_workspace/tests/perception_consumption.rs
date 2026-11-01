#![cfg(test)]
use super::*;
use crate::cognition::working_memory::{AcceptedPage, PerceptionGap, PerceptionRef};

fn line(seq: u64) -> PerceptionRef {
    PerceptionRef {
        arrival_seq: seq,
        event: Uuid::new_v4(),
        room: Uuid::new_v4(),
        author: Uuid::new_v4(),
    }
}

// what this catches: an accepted line that is lost on restart, woken before its snapshot is saved,
// accepted twice on replay, or accepted into a replaced memory; and a gap that is forgotten (P2,
// Codex #5691 773d34a4). Accept, restart, replay, retire: the cursor, the line and its wake survive
// exactly once, and a stale incarnation is refused.
#[tokio::test]
async fn accepted_perception_survives_restart_exactly_once(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let _native = crate::paths::NativeHomeOverride::install(home.path());
    let registry = Arc::new(PersonaWorkspaceRegistry::new());
    let owner = Uuid::new_v4();
    registry.register_from_cfg(cfg_for(owner))?;
    let memory = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    let incarnation = memory.incarnation();

    registry.accept_perception(
        owner,
        incarnation,
        AcceptedPage::Gap(PerceptionGap::UnvouchedImport),
    )?;
    let (a, b) = (line(1), line(3));
    let page = AcceptedPage::Rows {
        rows: vec![a, b],
        scanned_through: 3,
    };
    registry.accept_perception(owner, incarnation, page.clone())?;
    assert_eq!(
        memory.pending_perception_lines(),
        vec![a, b],
        "published only after the saved snapshot"
    );
    registry.accept_perception(owner, incarnation, page)?; // a lost reply replays
    assert_eq!(
        memory.pending_perception_lines().len(),
        2,
        "a replayed page is inert"
    );
    assert!(
        matches!(
            registry.accept_perception(
                owner,
                Uuid::new_v4(),
                AcceptedPage::Rows {
                    rows: vec![line(4)],
                    scanned_through: 4
                }
            ),
            Err(PerceptionConsumptionError::Replaced(_))
        ),
        "a replaced incarnation cannot accept"
    );

    registry.register_from_cfg(cfg_for(owner))?; // restart: restored from the checkpoint
    let restored = Arc::clone(&registry.cycles.lock()[&owner].working_memory);
    assert_eq!(
        restored.perception_cursor(),
        (3, true),
        "cursor and the unvouched receipt survive"
    );
    assert_eq!(
        restored.pending_perception_lines(),
        vec![a, b],
        "the accepted lines survive, wakes intact"
    );

    registry.record_perception_reads(owner, restored.incarnation(), &[1])?;
    assert_eq!(
        restored.pending_perception_lines(),
        vec![b],
        "only a shown line retires"
    );
    assert!(
        registry
            .record_perception_reads(owner, restored.incarnation(), &[9])
            .is_err(),
        "an unknown line cannot retire"
    );
    Ok(())
}
