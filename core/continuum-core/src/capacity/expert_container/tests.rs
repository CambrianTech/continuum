#![cfg(test)]
    use super::fixtures::{write_container, write_tiered_container};
    use super::*;

    #[test]
    fn native_numeric_manifest_preserves_producer_encoding() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 4);
        manifest.fmt = ContainerFormat::Native(2);
        let path = dir.path().join("manifest.json");
        let raw = serde_json::to_vec(&manifest).expect("serialize manifest");
        std::fs::write(&path, &raw).expect("native manifest");
        let mut container = ExpertContainer::open(dir.path()).expect("numeric format");
        assert_eq!(container.manifest().fmt, ContainerFormat::Native(2));
        assert_eq!(container.tiers()[0].quant, "IQ2");
        assert_eq!(
            serde_json::to_value(container.manifest()).expect("value")["fmt"],
            2
        );
        let mut record = vec![0; manifest.record_bytes as usize];
        container
            .fetch(ExpertKey::sharp(0, 0), &mut record)
            .expect("record envelope");
        assert_eq!(std::fs::read(path).expect("manifest unchanged"), raw);
        // This exercises the envelope, not native IQ2 payload decoding.
        assert_eq!(
            serde_json::to_value(ContainerFormat::from("IQ2")).expect("label"),
            "IQ2"
        );
        assert!(serde_json::from_str::<ContainerFormat>("-1").is_err());
    }

    #[test]
    fn whole_container_observation_refuses_partial_native_production() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_container(dir.path(), 2, 4);
        std::fs::write(dir.path().join("producer.trace"), b"original provenance").expect("trace");
        let observed = ExpertContainer::observe_complete(dir.path()).expect("complete");
        assert_eq!(
            observed.manifest_bytes,
            std::fs::read(dir.path().join("manifest.json")).expect("producer bytes")
        );
        assert_eq!(observed.bank_bytes, manifest.record_bytes * 8);
        assert_eq!(observed.files.len(), 4);
        assert!(observed
            .files
            .iter()
            .any(|file| file.name == "producer.trace"));
        assert_eq!(
            observed.total_bytes,
            observed.files.iter().map(|file| file.bytes).sum::<u64>()
        );
        let last_bank = dir.path().join("experts-L1.bin");
        std::fs::OpenOptions::new()
            .write(true)
            .open(&last_bank)
            .expect("bank")
            .set_len(manifest.record_bytes)
            .expect("partial write");
        assert!(ExpertContainer::observe_complete(dir.path()).is_err());
        // Lazy serving of an independent shard remains supported, but is not
        // evidence that the whole artifact can be migrated.
        assert!(ExpertContainer::open(dir.path()).is_ok());
        std::fs::remove_file(last_bank).expect("unfinished producer");
        assert!(ExpertContainer::observe_complete(dir.path()).is_err());
    }

    #[test]
    fn whole_container_never_counts_one_v1_bank_as_two_tiers() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 4);
        manifest.tiers = (0..2)
            .map(|id| TierSpec {
                id,
                quant: "IQ2".into(),
                record_bytes: manifest.record_bytes,
            })
            .collect();
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_vec(&manifest).expect("manifest"),
        )
        .expect("write");
        let error = ExpertContainer::observe_complete(dir.path()).expect_err("aliased tiers");
        assert!(error.to_string().contains("same bank"));
    }

    #[test]
    fn fetch_round_trips_every_expert_in_one_read() {
        // what this catches: offset arithmetic drift (the reuse=0 bug shape) —
        // every (layer, expert) must come back with ITS OWN header + payload.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 3, 4);
        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; c.manifest().record_bytes as usize];
        for layer in 0..3u16 {
            for expert in 0..4u16 {
                c.fetch(ExpertKey::sharp(layer, expert), &mut buf)
                    .expect("fetch");
                assert_eq!(buf[HEADER_IDENT_BYTES], (layer as u8) ^ 0xA0);
                assert_eq!(buf[HEADER_IDENT_BYTES + 1], expert as u8);
            }
        }
    }

    #[test]
    fn geometry_violations_fail_loud_at_open_not_at_read() {
        // what this catches: silent tolerance of a truncated bank or
        // misaligned record size — both must refuse before any fetch.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 4);

        // Truncated bank: chop one record off.
        let bank = dir.path().join("experts-L0.bin");
        let full = std::fs::read(&bank).expect("read bank");
        std::fs::write(&bank, &full[..full.len() - RECORD_ALIGN as usize]).expect("truncate");
        let mut c = ExpertContainer::open(dir.path()).expect("manifest still fine");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch(ExpertKey::sharp(0, 0), &mut buf)
            .expect_err("truncated bank must refuse");
        assert!(matches!(err, ContainerError::BankGeometry { .. }), "{err}");

        // Misaligned record_bytes: manifest itself must refuse.
        manifest.record_bytes = RECORD_ALIGN + 512;
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite manifest");
        let err = ExpertContainer::open(dir.path()).expect_err("misaligned must refuse");
        assert!(
            matches!(err, ContainerError::RecordMisaligned { .. }),
            "{err}"
        );
    }

    #[test]
    fn record_identity_mismatch_is_an_error_not_a_wrong_answer() {
        // what this catches: a drifted offset table serving expert A's bytes
        // for expert B — the identity check must turn that into an ERROR.
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 1, 2);
        // Corrupt record 1's header to claim it is expert 0.
        let bank = dir.path().join("experts-L0.bin");
        let mut bytes = std::fs::read(&bank).expect("read");
        let off = RECORD_ALIGN as usize + 6;
        bytes[off..off + 2].copy_from_slice(&0u16.to_le_bytes());
        std::fs::write(&bank, &bytes).expect("write");

        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch(ExpertKey::sharp(0, 1), &mut buf)
            .expect_err("identity mismatch must refuse");
        assert!(
            matches!(err, ContainerError::RecordIdentity { .. }),
            "{err}"
        );
    }

    #[test]
    fn ecache_over_container_skips_disk_on_a_recurring_working_set() {
        // what this catches: the #268/#269 seam composing wrong — with a
        // budget above the cliff, the SECOND pass over the same working set
        // must hit cache and issue ZERO container reads. This is the whole
        // point of the lane: reuse > 0, measured, not assumed.
        use super::super::expert_ecache::{EcacheBudget, EvictionPolicy, ExpertEcache};

        let dir = tempfile::tempdir().expect("tempdir");
        let manifest = write_container(dir.path(), 2, 8);
        let mut container = ExpertContainer::open(dir.path()).expect("open");

        let budget = EcacheBudget::derive(
            32 * RECORD_ALIGN, // 32 records available >> 2-per-token cliff
            manifest.activated_per_token,
            manifest.record_bytes,
        )
        .expect("above the cliff");
        let mut cache = ExpertEcache::new(budget, EvictionPolicy::Lfru);

        let working_set: Vec<ExpertKey> = (0..2u16)
            .flat_map(|layer| (0..8u16).map(move |expert| ExpertKey::sharp(layer, expert)))
            .collect();
        let mut buf = vec![0u8; manifest.record_bytes as usize];
        let mut disk_reads = 0usize;
        for pass in 0..2 {
            for &key in &working_set {
                if !cache.touch(key) {
                    container.fetch(key, &mut buf).expect("fetch");
                    disk_reads += 1;
                }
            }
            if pass == 0 {
                assert_eq!(disk_reads, working_set.len(), "cold pass reads each once");
            }
        }
        assert_eq!(
            disk_reads,
            working_set.len(),
            "warm pass must be served entirely from cache — zero new disk reads"
        );
    }

    // what this catches: the v2 tiered contract end-to-end — the reader mirror
    // of the packer's own 82-test round-trip. Every (layer, expert, tier) must
    // come back from ITS tier's bank at ITS tier's record size with ITS OWN
    // header + tier-distinguishable payload; offset = expert × tier.record_bytes.
    #[test]
    fn tiered_fetch_round_trips_every_expert_at_every_tier() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_tiered_container(dir.path(), 2, 3);
        let mut c = ExpertContainer::open(dir.path()).expect("open v2");
        assert_eq!(c.tiers().len(), 2);
        for tier in 0..2u16 {
            let record_bytes = c.tiers()[tier as usize].record_bytes;
            let mut buf = vec![0u8; record_bytes as usize];
            for layer in 0..2u16 {
                for expert in 0..3u16 {
                    c.fetch(
                        ExpertKey {
                            layer,
                            expert,
                            tier,
                        },
                        &mut buf,
                    )
                    .expect("tiered fetch");
                    assert_eq!(buf[HEADER_IDENT_BYTES], tier as u8 ^ 0x5A);
                    assert_eq!(buf[HEADER_IDENT_BYTES + 1], expert as u8);
                }
            }
        }
    }

    // what this catches: v1 stays the degenerate single-tier case — a raw v1
    // manifest JSON WITHOUT any `tiers` field parses, synthesizes one tier
    // from the top-level record_bytes, and the tier-less fetch() keeps
    // reading legacy `experts-L{n}.bin` banks. Back-compat is the contract,
    // not an accident.
    #[test]
    fn v1_manifest_without_tiers_field_reads_as_single_tier() {
        let dir = tempfile::tempdir().expect("tempdir");
        write_container(dir.path(), 1, 2);
        // Strip the serialized `tiers` field entirely — a pre-v2 foundry
        // never wrote one.
        let raw = std::fs::read_to_string(dir.path().join("manifest.json")).expect("read");
        let mut v: serde_json::Value = serde_json::from_str(&raw).expect("json");
        v.as_object_mut().expect("obj").remove("tiers");
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&v).expect("serialize"),
        )
        .expect("rewrite");

        let mut c = ExpertContainer::open(dir.path()).expect("open v1");
        assert_eq!(c.tiers().len(), 1, "v1 = one synthesized tier");
        assert_eq!(c.tiers()[0].record_bytes, RECORD_ALIGN);
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        c.fetch(ExpertKey::sharp(0, 1), &mut buf)
            .expect("legacy fetch path");
        assert_eq!(buf[HEADER_IDENT_BYTES + 1], 1);
    }

    // what this catches: tier indexing refusals — an out-of-range tier and a
    // tier table whose ids don't equal their index (order drift between
    // writer and reader) must both refuse loudly, never mis-read a bank.
    #[test]
    fn tier_range_and_id_order_violations_refuse() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_tiered_container(dir.path(), 1, 1);
        let mut c = ExpertContainer::open(dir.path()).expect("open");
        let mut buf = vec![0u8; RECORD_ALIGN as usize];
        let err = c
            .fetch(
                ExpertKey {
                    layer: 0,
                    expert: 0,
                    tier: 2,
                },
                &mut buf,
            )
            .expect_err("tier 2 of 2 must refuse");
        assert!(
            matches!(err, ContainerError::TierOutOfRange { .. }),
            "{err}"
        );

        manifest.tiers.swap(0, 1); // ids no longer equal their index
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite");
        let err = ExpertContainer::open(dir.path()).expect_err("id/index drift must refuse");
        assert!(
            matches!(err, ContainerError::TierIdMismatch { .. }),
            "{err}"
        );
    }

    #[test]
    fn unknown_manifest_version_is_refused() {
        // what this catches: a v2 foundry container silently misread by a v1
        // reader — version gates the whole open.
        let dir = tempfile::tempdir().expect("tempdir");
        let mut manifest = write_container(dir.path(), 1, 1);
        manifest.version = ExpertContainer::KNOWN_VERSION + 1;
        std::fs::write(
            dir.path().join("manifest.json"),
            serde_json::to_string(&manifest).expect("serialize"),
        )
        .expect("rewrite");
        let err = ExpertContainer::open(dir.path()).expect_err("future version must refuse");
        assert!(
            matches!(err, ContainerError::ManifestVersion { .. }),
            "{err}"
        );
    }
