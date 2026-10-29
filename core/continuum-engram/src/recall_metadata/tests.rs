#![cfg(test)]
    use super::*;

    #[test]
    fn new_registry_is_empty() {
        let r = RecallMetadataRegistry::new();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
    }

    #[test]
    fn admit_with_defaults_creates_neutral_entry() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        let before = EpochMs::now().millis();
        r.admit_with_defaults(id);
        let after = EpochMs::now().millis();
        let m = r.get(id).unwrap();
        // Salience/access/protected fields match Default; last_decayed_ms
        // is stamped to wallclock (so the first decay tick has a bounded
        // delta), so compare it separately as a range rather than ==.
        assert_eq!(m.salience, 0.5);
        assert_eq!(m.access_count, 0);
        assert_eq!(m.last_accessed_ms, 0);
        assert_eq!(m.protected_until_ms, 0);
        assert!(
            m.last_decayed_ms >= before && m.last_decayed_ms <= after,
            "last_decayed_ms ({}) should be within [{}, {}]",
            m.last_decayed_ms,
            before,
            after
        );
    }

    #[test]
    fn admit_overrides_default_metadata() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id);
        let custom = RecallMetadata {
            salience: 0.9,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 1000,
            last_decayed_ms: 0,
        };
        r.admit(id, custom);
        assert_eq!(r.get(id).unwrap(), custom);
    }

    #[test]
    fn record_recall_hit_increments_and_uplifts() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id);
        let before = r.get(id).unwrap();
        assert_eq!(before.salience, 0.5);

        r.record_recall_hit(id, 1_000_000);
        let after_one = r.get(id).unwrap();
        assert_eq!(after_one.access_count, 1);
        assert_eq!(after_one.last_accessed_ms, 1_000_000);
        // Salience should have grown but not by more than the cap (0.1)
        // per hit.
        assert!(after_one.salience > before.salience);
        assert!(after_one.salience <= before.salience + 0.1 + f32::EPSILON);

        // Two more hits — salience keeps growing with diminishing
        // returns, asymptoting toward the recall ceiling (not 1.0).
        r.record_recall_hit(id, 1_001_000);
        r.record_recall_hit(id, 1_002_000);
        let after_three = r.get(id).unwrap();
        assert_eq!(after_three.access_count, 3);
        assert!(after_three.salience > after_one.salience);
        assert!(after_three.salience <= 1.0);
    }

    // what this catches: recall FREQUENCY alone must not self-reinforce a memory into
    // the top band — the stale-belief spiral fix. Many hits asymptote to
    // RECALL_UPLIFT_CEILING, never above; and a memory already above the ceiling
    // (genuine importance) is neither lowered nor raised by recall. // regression:
    // live 2026-07-14 "workspace is empty" pinned to ~0.98 by re-surfacing
    #[test]
    fn recall_uplift_caps_at_the_ceiling_not_one() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id); // starts at 0.5
        for i in 0..50 {
            r.record_recall_hit(id, 1_000_000 + i * 1000);
        }
        let s = r.get(id).unwrap().salience;
        assert!(
            s <= RECALL_UPLIFT_CEILING + f32::EPSILON,
            "recall alone must not exceed the ceiling: {s}"
        );
        assert!(s > 0.85, "but it should still climb close to it: {s}");

        // A memory admitted ABOVE the ceiling keeps its value — recall neither lifts
        // nor lowers it (genuine importance owns the top band).
        let hi = Uuid::new_v4();
        let mut md = RecallMetadata::default();
        md.salience = 0.97;
        r.admit(hi, md);
        r.record_recall_hit(hi, 2_000_000);
        assert_eq!(
            r.get(hi).unwrap().salience,
            0.97,
            "recall must not disturb an already-important memory"
        );
    }

    #[test]
    fn record_recall_hit_creates_entry_if_absent() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        // No prior admit call.
        r.record_recall_hit(id, 12345);
        let m = r.get(id).unwrap();
        assert_eq!(m.access_count, 1);
        assert_eq!(m.last_accessed_ms, 12345);
    }

    #[test]
    fn apply_decay_reduces_salience_over_time() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        let m = RecallMetadata {
            salience: 0.8,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 0,
            // last_decayed_ms = 0; first decay tick at t=2h applies
            // 2h of decay.
            last_decayed_ms: 0,
        };
        r.admit(id, m);

        let two_hours_ms: u64 = 7_200_000;
        r.apply_decay(id, two_hours_ms);
        let decayed = r.get(id).unwrap();
        assert!(decayed.salience < 0.8, "got {}", decayed.salience);
        assert!(decayed.salience > 0.0);
        // last_decayed_ms advanced to now_ms.
        assert_eq!(decayed.last_decayed_ms, two_hours_ms);
    }

    #[test]
    fn apply_decay_skips_protected_engrams() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        let m = RecallMetadata {
            salience: 0.8,
            access_count: 0,
            last_accessed_ms: 0,
            protected_until_ms: 100_000_000_000,
            last_decayed_ms: 0,
        };
        r.admit(id, m);

        // Try to decay during protection window. Should be no-op.
        r.apply_decay(id, 1_000_000);
        let after = r.get(id).unwrap();
        assert_eq!(
            after.salience, 0.8,
            "protection window failed to prevent decay"
        );
    }

    #[test]
    fn high_salience_decays_slower_than_low() {
        let r = RecallMetadataRegistry::new();
        let low_id = Uuid::new_v4();
        let high_id = Uuid::new_v4();
        r.admit(
            low_id,
            RecallMetadata {
                salience: 0.0,
                ..Default::default()
            },
        );
        r.admit(
            high_id,
            RecallMetadata {
                salience: 1.0,
                ..Default::default()
            },
        );

        let one_hour_ms: u64 = 3_600_000;
        r.apply_decay(low_id, one_hour_ms);
        r.apply_decay(high_id, one_hour_ms);
        let low_after = r.get(low_id).unwrap();
        let high_after = r.get(high_id).unwrap();
        assert!(low_after.salience < 0.5);
        assert!(
            high_after.salience > 0.7,
            "high-salience decayed too fast: {}",
            high_after.salience
        );
    }

    #[test]
    fn apply_decay_twice_with_overlapping_windows_is_safe() {
        // Reviewer-defect-driven: prove the double-decay defect is
        // structurally impossible. Two ticks with overlapping
        // "now" deltas should NOT produce 2× decay; the second tick
        // simply observes the shortened remaining delta.
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit(
            id,
            RecallMetadata {
                salience: 0.8,
                last_decayed_ms: 0,
                ..Default::default()
            },
        );
        // First tick at t=2h.
        r.apply_decay(id, 7_200_000);
        let after_first = r.get(id).unwrap();
        // Second tick at t=2h (same instant — double-fire).
        r.apply_decay(id, 7_200_000);
        let after_second = r.get(id).unwrap();
        assert_eq!(
            after_first.salience, after_second.salience,
            "double-fire at same now_ms should be a no-op (delta=0)"
        );
    }

    #[test]
    fn evict_removes_metadata() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit_with_defaults(id);
        assert!(r.get(id).is_some());
        let removed = r.evict(id);
        assert!(removed.is_some());
        assert!(r.get(id).is_none());
    }

    #[test]
    fn clone_shares_inner() {
        let r1 = RecallMetadataRegistry::new();
        let r2 = r1.clone();
        let id = Uuid::new_v4();
        r1.admit_with_defaults(id);
        // r2 should see the same entry — they share Arc<DashMap>.
        assert!(r2.get(id).is_some());
        assert_eq!(r2.len(), 1);
    }

    #[test]
    fn decay_clamps_at_salience_floor_never_disappears() {
        // Joel's trauma test: "Will the hippocampus just decay away?"
        // The substrate guarantees: no, salience floors at
        // SALIENCE_FLOOR regardless of elapsed time. Memory drains;
        // it does not erase.
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.admit(
            id,
            RecallMetadata {
                salience: 0.5, // default admission
                last_decayed_ms: 0,
                ..Default::default()
            },
        );
        // Apply a YEAR of decay. Under the old (no-floor) formula,
        // salience would underflow to 0. With the floor it stays at
        // SALIENCE_FLOOR.
        let one_year_ms: u64 = 365 * 24 * 3_600_000;
        r.apply_decay(id, one_year_ms);
        let after = r.get(id).unwrap();
        assert_eq!(
            after.salience, SALIENCE_FLOOR,
            "salience should clamp at the floor, not drain to zero"
        );
    }

    #[test]
    fn pin_permanent_blocks_all_decay() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        // Admit normally, then pin.
        r.admit_with_defaults(id);
        r.pin_permanent(id);
        let after_pin = r.get(id).unwrap();
        assert_eq!(after_pin.protected_until_ms, PERMANENT_PROTECTION);
        assert_eq!(after_pin.salience, 1.0);

        // Even a million-year decay attempt is a no-op.
        let ridiculous_time_ms: u64 = 1_000_000 * 365 * 24 * 3_600_000;
        r.apply_decay(id, ridiculous_time_ms);
        let after_decay = r.get(id).unwrap();
        assert_eq!(
            after_decay.salience, 1.0,
            "permanent pin must protect forever"
        );
        assert_eq!(after_decay.protected_until_ms, PERMANENT_PROTECTION);
    }

    #[test]
    fn pin_permanent_creates_entry_if_absent() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        // No prior admission.
        r.pin_permanent(id);
        let m = r.get(id).unwrap();
        assert_eq!(m.salience, 1.0);
        assert_eq!(m.protected_until_ms, PERMANENT_PROTECTION);
    }

    #[test]
    fn unpin_restores_normal_decay() {
        let r = RecallMetadataRegistry::new();
        let id = Uuid::new_v4();
        r.pin_permanent(id);
        r.unpin(id);
        let after_unpin = r.get(id).unwrap();
        assert_eq!(after_unpin.protected_until_ms, 0);
        // Salience preserved at 1.0 (unpin doesn't reset salience).
        assert_eq!(after_unpin.salience, 1.0);

        // After unpinning, decay applies normally — but the floor
        // still protects. So after a long delay, salience drops to
        // the floor.
        let long_time_ms: u64 = 30 * 24 * 3_600_000; // 30 days
        r.apply_decay(id, long_time_ms);
        let after = r.get(id).unwrap();
        assert!(
            after.salience >= SALIENCE_FLOOR,
            "even unpinned + heavily-decayed engrams stay above the floor"
        );
    }

    #[test]
    fn engram_ids_returns_all_tracked() {
        let r = RecallMetadataRegistry::new();
        let ids: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
        for id in &ids {
            r.admit_with_defaults(*id);
        }
        let listed = r.engram_ids();
        assert_eq!(listed.len(), 5);
        for id in &ids {
            assert!(listed.contains(id));
        }
    }

    // ── EngramRecallMetadata persistence tests (#168) ────────────

    /// What this catches: the round-trip conversion between the
    /// hot-path `(engram_id, RecallMetadata)` pair and the
    /// persistence-side `EngramRecallMetadata` preserves every field
    /// exactly. The boundary is the most likely place for drift —
    /// a new field on RecallMetadata without a corresponding field on
    /// the persistence row would silently lose data at flush.
    #[test]
    fn engram_recall_metadata_lifts_and_lowers_losslessly() {
        let engram_id = Uuid::new_v4();
        let m = RecallMetadata {
            salience: 0.73,
            access_count: 9,
            last_accessed_ms: 1_700_000_000_000,
            protected_until_ms: 1_700_000_300_000,
            last_decayed_ms: 1_700_000_010_000,
        };
        let row = EngramRecallMetadata::for_new_row(engram_id, m);
        assert_eq!(row.engram_id, engram_id);
        assert_eq!(row.salience, m.salience);
        assert_eq!(row.access_count, m.access_count);
        assert_eq!(row.last_accessed_ms, m.last_accessed_ms);
        assert_eq!(row.protected_until_ms, m.protected_until_ms);
        assert_eq!(row.last_decayed_ms, m.last_decayed_ms);

        let (back_id, back_m): (Uuid, RecallMetadata) = row.into();
        assert_eq!(back_id, engram_id);
        assert_eq!(back_m, m);
    }

    /// What this catches: the derived schema has BaseEntity columns +
    /// every domain field including engram_id, salience, and the
    /// timestamp/counter trio. Drift between the Rust struct and the
    /// schema becomes visible here at the moment a field gets added
    /// to one but not the other.
    #[test]
    fn engram_recall_metadata_schema_has_expected_columns() {
        use continuum_kernel::orm::OrmEntity;
        let schema = EngramRecallMetadata::collection_schema();
        assert_eq!(schema.collection, "engram_recall_metadata");
        let names: std::collections::BTreeSet<&str> =
            schema.fields.iter().map(|f| f.name.as_str()).collect();
        for required in [
            "id",
            "createdAt",
            "updatedAt",
            "version",
            "engramId",
            "salience",
            "accessCount",
            "lastAccessedMs",
            "protectedUntilMs",
            "lastDecayedMs",
        ] {
            assert!(
                names.contains(required),
                "missing column {required:?}; have {names:?}"
            );
        }
    }

    /// What this catches: the engram_id field carries the foreign-key
    /// reference to engrams.id with ON DELETE CASCADE. If the FK is
    /// dropped or the cascade rule changes, this test screams. Per
    /// [[no-fallbacks-ever]] extended to relational invariants —
    /// the cascade IS the invariant.
    #[test]
    fn engram_recall_metadata_carries_fk_to_engrams_with_cascade() {
        use continuum_kernel::orm::types::CascadeRule;
        use continuum_kernel::orm::OrmEntity;
        let schema = EngramRecallMetadata::collection_schema();
        let engram_id_field = schema
            .fields
            .iter()
            .find(|f| f.name == "engramId")
            .expect("engramId field must be present");
        let fk = engram_id_field
            .foreign_key
            .as_ref()
            .expect("engramId must carry a foreign_key");
        assert_eq!(fk.collection, "engrams");
        assert_eq!(fk.field, "id");
        assert_eq!(fk.on_delete, CascadeRule::Cascade);
        assert!(
            engram_id_field.unique,
            "engramId must be UNIQUE for the 1:1 invariant"
        );
        assert!(engram_id_field.indexed);
    }

    /// What this catches: end-to-end relational round-trip across
    /// two derived entities. An Engram parent persists; a child
    /// EngramRecallMetadata references it via FK; deleting the
    /// parent CASCADE-wipes the child at the DB layer. The proof
    /// that the substrate's persistence is now genuinely relational.
    #[tokio::test]
    async fn engram_recall_metadata_cascade_deletes_with_engram() {
        use continuum_kernel::orm::adapter::StorageAdapter;
        use continuum_kernel::orm::sqlite::SqliteAdapter;
        use continuum_kernel::orm::OrmStore;
        use crate::engram::{AircMessageRef, Engram, EngramKind, EngramOrigin, TrustState};
        use std::sync::Arc;

        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("engrams.sqlite");
        let adapter = SqliteAdapter::open(&path).await.expect("adapter open");
        let adapter: Arc<dyn StorageAdapter> = Arc::new(adapter);

        let engrams = OrmStore::<Engram>::new(Arc::clone(&adapter))
            .await
            .expect("engrams store");
        let metadata = OrmStore::<EngramRecallMetadata>::new(Arc::clone(&adapter))
            .await
            .expect("metadata store");

        let engram = Engram {
            context_id: None,
            id: Uuid::new_v4(),
            kind: EngramKind::Episodic,
            content: "anchor".to_string(),
            origin: EngramOrigin::Airc(AircMessageRef {
                transport: "airc".to_string(),
                room_id: "general".to_string(),
                message_id: "msg-cascade".to_string(),
                sender_id: "airc-test".to_string(),
                sent_at_ms: 1_000,
                received_at_ms: 1_000,
                content_hash: "sha256:cascade".to_string(),
                signature: "sig-cascade".to_string(),
                proof_refs: vec![],
                schema_version: "v1".to_string(),
                client_name: Some("test".to_string()),
            }),
            recall_keys: vec![],
            admitted_at_ms: 1_000,
            trust_state_at_admission: TrustState::ApprovedPeer,
            admission_trace_id: None,
        };
        let engram_id = engram.id;
        engrams.save(engram_id, &engram).await.expect("save engram");

        let row = EngramRecallMetadata::for_new_row(
            engram_id,
            RecallMetadata {
                salience: 0.7,
                access_count: 1,
                last_accessed_ms: 1_000,
                protected_until_ms: 0,
                last_decayed_ms: 1_000,
            },
        );
        let row_id = Uuid::parse_str(&row.base.id).expect("base id parses");
        metadata.save(row_id, &row).await.expect("save metadata");

        // Sanity: metadata row is findable.
        assert!(metadata
            .find_by_id(row_id)
            .await
            .expect("find_by_id pre-delete")
            .is_some());

        // Delete the engram. SQLite's CASCADE rule must remove the
        // child metadata row at the DB layer.
        let deleted = engrams.delete(engram_id).await.expect("delete engram");
        assert!(deleted);

        let after = metadata
            .find_by_id(row_id)
            .await
            .expect("find_by_id post-delete");
        assert!(
            after.is_none(),
            "ON DELETE CASCADE must wipe the recall-metadata row when its engram is deleted"
        );
    }

    // what this catches: the supersession demotion contract (#221 slice 2) — a
    // demoted belief drops to the FLOOR immediately (not a gradual decay) and
    // loses its protection window, but the ROW survives (the floor guarantee:
    // memory drains, never disappears). If demote_to_floor ever left protection
    // intact, a freshly-admitted stale belief would shrug off its own
    // supersession for the whole protection window.
    #[test]
    fn demote_to_floor_floors_salience_and_clears_protection() {
        let reg = RecallMetadataRegistry::new();
        let id = Uuid::from_u128(7);
        reg.admit(
            id,
            RecallMetadata {
                salience: 0.9,
                access_count: 3,
                last_accessed_ms: 1_000,
                protected_until_ms: u64::MAX, // even a protected belief demotes
                last_decayed_ms: 1_000,
            },
        );
        reg.demote_to_floor(id, 5_000);
        let m = reg.get(id).expect("row survives demotion");
        assert_eq!(m.salience, SALIENCE_FLOOR, "floored, not deleted");
        assert_eq!(m.protected_until_ms, 0, "protection cleared");
        assert_eq!(m.last_decayed_ms, 5_000, "decay clock reset at demotion");
        // Unknown id: no panic, no phantom row.
        reg.demote_to_floor(Uuid::from_u128(999), 5_000);
        assert!(reg.get(Uuid::from_u128(999)).is_none());
    }
