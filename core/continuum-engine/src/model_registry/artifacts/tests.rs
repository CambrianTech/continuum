#![cfg(test)]
    // what this catches: card e8a15ab0. A training request naming a served model by its HF
    // base (Kimi's restored request: `Qwen/Qwen3.8-27B`) must match the lane serving that row
    // (`ggml-org/Qwen3.8-27B-GGUF`); an exact string compare filtered the in-engine trainer
    // out and left a separate trainer waiting on 29.9 GB that never frees. And it must not
    // over-match: another base, or no registry, is not the same base.
    #[test]
    fn a_request_naming_the_served_rows_hf_base_names_that_base() {
        const SERVED: &str = "ggml-org/Qwen3.8-27B-GGUF";
        let row = crate::model_registry::catalog::models()
            .into_iter()
            .find(|m| m.id == SERVED)
            .expect("the catalog serves Qwen3.8-27B");
        let hf = row.hf_source.clone().expect("the row declares its trainable base");
        let registry = crate::model_registry::Registry::from_catalog(vec![row], crate::model_registry::catalog::providers())
            .expect("one catalog row loads");
        assert!(names_the_same_base_in(Some(&registry), SERVED, SERVED), "the catalog id is its own base");
        assert!(names_the_same_base_in(Some(&registry), SERVED, &hf), "the row's HF base names the same base");
        assert!(!names_the_same_base_in(Some(&registry), SERVED, "Qwen/Qwen3-32B"), "another base does not");
        assert!(!names_the_same_base_in(None, SERVED, &hf), "with no registry only the exact id matches");
    }

    // what this catches: the embedded-MTP capability signal lying in either
    // direction. A false positive would push `--spec-type draft-mtp` onto a
    // model with no nextn head (llama-server may refuse or misbehave); a false
    // negative silently forfeits the speculation the head was trained for.
    // Synthetic GGUF headers, no real weights — the scan is header-only by
    // design and must stay that way (it runs at model resolution).
    #[test]
    fn embedded_mtp_detection_reads_headers_not_weights() {
        use std::io::Write;
        fn gguf_with_tensor(name: &str) -> std::path::PathBuf {
            let dir = std::env::temp_dir().join(format!("mtp-det-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&dir).unwrap(); // unwrap: test fixture tempdir must exist
            let p = dir.join("model.gguf");
            let mut f = std::fs::File::create(&p).unwrap();
            f.write_all(b"GGUF").unwrap();
            f.write_all(&3u32.to_le_bytes()).unwrap(); // version
            f.write_all(&1u64.to_le_bytes()).unwrap(); // n_tensors
            f.write_all(&1u64.to_le_bytes()).unwrap(); // n_kv
            // one KV: key "k" -> u32 7
            f.write_all(&1u64.to_le_bytes()).unwrap();
            f.write_all(b"k").unwrap();
            f.write_all(&4u32.to_le_bytes()).unwrap(); // type u32
            f.write_all(&7u32.to_le_bytes()).unwrap();
            // one tensor: name, ndims=1, dim, dtype, offset
            f.write_all(&(name.len() as u64).to_le_bytes()).unwrap();
            f.write_all(name.as_bytes()).unwrap();
            f.write_all(&1u32.to_le_bytes()).unwrap();
            f.write_all(&8u64.to_le_bytes()).unwrap();
            f.write_all(&0u32.to_le_bytes()).unwrap();
            f.write_all(&0u64.to_le_bytes()).unwrap();
            p
        }
        let with = gguf_with_tensor("blk.40.nextn.eh_proj.weight");
        let without = gguf_with_tensor("blk.40.attn_q.weight");
        assert!(super::gguf_has_embedded_mtp(&with), "nextn tensor must be detected");
        assert!(!super::gguf_has_embedded_mtp(&without), "plain model must not");
        assert!(
            !super::gguf_has_embedded_mtp(std::path::Path::new("/nonexistent.gguf")),
            "unreadable file is honestly false, never a panic"
        );
        for p in [with, without] {
            let _ = std::fs::remove_dir_all(p.parent().unwrap());
        }
    }


    use super::*;
    use crate::model_registry::types::{Arch, Capability};
    use std::collections::BTreeSet;

    fn model(id: &str, hint: Option<&str>, explicit: Option<PathBuf>) -> Model {
        Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".into(),
            arch: Arch::Qwen35,
            context_window: 262144,
            max_output_tokens: 32768,
            tokens_per_second: 33.0,
            capabilities: BTreeSet::from([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
            ]),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: hint.map(str::to_string),
            hf_source: None,
            gguf_local_path: explicit,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: Default::default(),
            stop_sequences: Vec::new(),
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
            serving: Default::default(), // test/fixture literal: substrate defaults (text-only main lane, unverified kv-shift)
        }
    }

    // what this catches: #106 live spawn failure 2026-08-02 — pulling Qwen2.5-VL
    // wrote the mmproj AFTER the main weights, mtime-newest candidate selection
    // picked the PROJECTOR as the model, and llama-server died with "unsupported
    // model architecture: 'clip'" (serving fully down until unpin). A `*mmproj*`
    // GGUF must never be main-model candidate no matter its mtime; the
    // mmproj-POSITIVE scan (`find_mmproj_beside`) must still find it.
    #[test]
    fn mmproj_companion_never_wins_main_model_resolution() {
        let snap = tempfile::tempdir().unwrap();
        let model_dir = snap.path().join("snapshots").join("abc123");
        std::fs::create_dir_all(&model_dir).unwrap();
        let main = model_dir.join("Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf");
        write_empty_gguf(&main);
        // Written SECOND → newer mtime, the exact live download ordering.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mmproj = model_dir.join("mmproj-Qwen2.5-VL-7B-Instruct-Q8_0.gguf");
        write_empty_gguf(&mmproj);

        let picked = find_ggufs_under_snapshots(snap.path()).expect("main model resolves");
        assert_eq!(picked, main, "projector must not out-mtime the model");
        let proj = find_mmproj_beside(&model_dir).expect("projector still discoverable");
        assert_eq!(proj, mmproj);
        // A directory holding ONLY a projector resolves no main model — blind
        // beats serving CLIP as a mind.
        std::fs::remove_file(&main).unwrap();
        assert!(find_ggufs_under_snapshots(snap.path()).is_none());
    }

    // what this catches: #440 — the ggml-org Qwen3.8 snapshot ships main + `mtp-*.gguf`
    // draft head + mmproj in ONE dir, and the draft downloads AFTER the main weights
    // (live ordering 2026-08-15: main 02:48, mtp 02:49). Without the mtp exclusion,
    // mtime-newest candidate selection serves the 1.6GB DRAFT HEAD as the 27B model —
    // the exact #106 clip failure shape. The draft-POSITIVE scan
    // (`find_mtp_draft_beside`) must still find it so the spawn can pass
    // `--spec-type draft-mtp`.
    #[test]
    fn mtp_draft_head_never_wins_main_model_resolution_but_resolves_as_draft() {
        let snap = tempfile::tempdir().unwrap();
        let model_dir = snap.path().join("snapshots").join("qwen38");
        std::fs::create_dir_all(&model_dir).unwrap();
        let main = model_dir.join("Qwen3.8-27B-Q4_K_M.gguf");
        write_empty_gguf(&main);
        // Written SECOND → newer mtime, the exact live download ordering.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let draft = model_dir.join("mtp-Qwen3.8-27B-Q4_0.gguf");
        write_empty_gguf(&draft);

        let picked = find_ggufs_under_snapshots(snap.path()).expect("main model resolves");
        assert_eq!(picked, main, "draft head must not out-mtime the model");
        let found = find_mtp_draft_beside(&model_dir).expect("draft head still discoverable");
        assert_eq!(found, draft);
        // A directory with no mtp sibling resolves no draft — the spawn adds no
        // spec-decode flags and serving is byte-identical to pre-#440.
        std::fs::remove_file(&draft).unwrap();
        assert!(find_mtp_draft_beside(&model_dir).is_none());
    }

    // what this catches: tier-1 resolution — an explicitly declared projector resolves
    // (with `~` expansion + existence check) so the serving spawn passes `--mmproj` and
    // the model can SEE; a declared-but-absent projector with no GGUF-sibling either,
    // or no projector at all, resolves to None so a Vision row can't silently claim
    // sight it can't deliver (the spawn's loud-warn path). Runs under an empty test HOME
    // so tier-2's `resolve_gguf` finds nothing — the None cases stay deterministic and
    // don't leak a real on-disk projector from the dev machine's HF cache (cf. #72).
    #[test]
    fn resolves_declared_mmproj_only_when_the_projector_file_exists() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let dir = tempfile::tempdir().unwrap();
            let mmproj = dir.path().join("mmproj-f16.gguf");
            fs::write(&mmproj, b"\0").unwrap();

            let mut m = model("qwen-vl", None, None);
            m.mmproj_local_path = Some(mmproj.clone());
            assert_eq!(
                resolve_mmproj_for_model(&m).as_deref(),
                Some(mmproj.as_path())
            );

            // Declared but not on disk, and no GGUF resolves (empty HOME) → None
            // (serving warns TEXT-ONLY, never fakes sight).
            m.mmproj_local_path = Some(dir.path().join("absent-mmproj.gguf"));
            assert!(resolve_mmproj_for_model(&m).is_none());

            // No projector declared, no GGUF → None.
            m.mmproj_local_path = None;
            assert!(resolve_mmproj_for_model(&m).is_none());
        });
    }

    // what this catches: tier-2 self-provisioning — a VL model's mmproj auto-resolves
    // from the SAME HF cache snapshot as its GGUF, with NO declared-local path. The
    // projector ships beside the GGUF in `*-GGUF` repos, so pulling the model into the
    // HF cache is enough; there is no separate hand-placement step for the projector.
    // This is the resolution asymmetry fix: the mmproj now resolves like the GGUF
    // (local → HF cache), so vision serving is managed, not manually wired.
    #[test]
    fn resolves_mmproj_from_the_gguf_hf_cache_snapshot_when_not_declared() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let snap = home.path().join(
                ".cache/huggingface/hub/models--continuum-ai--qwen3-vl-7b-GGUF/snapshots/abc",
            );
            fs::create_dir_all(&snap).unwrap();
            let gguf = snap.join("qwen3-vl-7b-Q4_K_M.gguf");
            write_empty_gguf(&gguf);
            let mmproj = snap.join("mmproj-qwen3-vl-7b-f16.gguf");
            write_empty_gguf(&mmproj);

            // GGUF resolves from the HF cache via the hint; the projector is NOT declared
            // locally (declared path is stale/absent) but sits beside the GGUF.
            let mut m = model("qwen3-vl-7b", Some("continuum-ai/qwen3-vl-7b-GGUF"), None);
            m.mmproj_local_path = Some(PathBuf::from("~/nonexistent/mmproj.gguf"));
            assert_eq!(
                resolve_mmproj_for_model(&m).as_deref(),
                Some(mmproj.as_path()),
                "mmproj must resolve beside the GGUF in the HF cache snapshot"
            );

            // Tier 1 still wins when a declared projector is actually present.
            m.mmproj_local_path = Some(mmproj.clone());
            assert_eq!(
                resolve_mmproj_for_model(&m).as_deref(),
                Some(mmproj.as_path())
            );
        });
    }

    #[test]
    fn resolves_huggingface_cache_from_hint_when_explicit_path_is_stale() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let cached = home.path().join(
                ".cache/huggingface/hub/models--continuum-ai--qwen3.5-4b-code-forged-GGUF/snapshots/abc",
            );
            fs::create_dir_all(&cached).unwrap();
            let gguf = cached.join("qwen3.5-4b-code-forged-Q4_K_M.gguf");
            write_empty_gguf(&gguf);

            let resolved = resolve_gguf_for_model(&model(
                "continuum-ai/qwen3.5-4b-code-forged-GGUF",
                Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
                Some(PathBuf::from("~/missing/docker/bundle/model.gguf")),
            ));

            assert_eq!(resolved.as_deref(), Some(gguf.as_path()));
        });
    }

    #[test]
    fn explicit_existing_path_wins() {
        let home = tempfile::tempdir().unwrap();
        with_test_home(home.path(), || {
            let explicit = home.path().join("models").join("model.gguf");
            fs::create_dir_all(explicit.parent().unwrap()).unwrap();
            write_empty_gguf(&explicit);
            let resolved = resolve_gguf_for_model(&model(
                "continuum-ai/qwen3.5-4b-code-forged-GGUF",
                Some("huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf"),
                Some(PathBuf::from("~/models/model.gguf")),
            ));
            assert_eq!(resolved.as_deref(), Some(explicit.as_path()));
        });
    }

    // what this catches: local model-dir resolution disambiguates by size WITHOUT
    // the deleted `["14b","32b","4b",...]` table or `qwen`/`compacted` family
    // strings. A `19b` request must pick the `19b` dir and reject the `32b` dir
    // purely via token-subset matching — and it must work for a NON-qwen family,
    // proving no hardcoded model knowledge leaked back in.
    #[test]
    fn find_model_dir_disambiguates_size_without_a_hardcoded_table() {
        let root = tempfile::tempdir().unwrap();
        for dir in [
            "qwen3-coder-30b-a3b-compacted-32b",
            "qwen3-coder-30b-a3b-compacted-19b",
            "llama-3-8b-instruct",
        ] {
            let d = root.path().join(dir);
            fs::create_dir_all(&d).unwrap();
            write_empty_gguf(&d.join("model-Q4_K_M.gguf"));
        }

        let resolved = find_model_dir_in_root(
            "Continuum/qwen3-coder-30b-a3b-compacted-19b-256k",
            root.path(),
        );
        assert_eq!(
            resolved.as_deref(),
            Some(
                root.path()
                    .join("qwen3-coder-30b-a3b-compacted-19b")
                    .as_path()
            ),
            "19b request must select the 19b dir, not the 32b sibling"
        );

        // Family-agnostic: a llama request resolves its own dir with zero qwen logic.
        let llama = find_model_dir_in_root("meta/llama-3-8b-instruct", root.path());
        assert_eq!(
            llama.as_deref(),
            Some(root.path().join("llama-3-8b-instruct").as_path())
        );

        // A size the request does not name has no subset dir → no false match.
        let absent = find_model_dir_in_root("Continuum/qwen3-coder-70b", root.path());
        assert_eq!(
            absent, None,
            "no 70b dir exists; must not match a 32b/19b sibling"
        );
    }

    // what this catches: the 2026-08-29 overnight killer — a 28-shard split GGUF
    // sized by shard 1 alone (0.69 of 79 GB), which poisoned kv_per_token and let
    // the planner size a window past the verified geometry (six hours of silent
    // Metal OOMs). Splits sum ALL siblings; singles stay their own size; a
    // non-split name with "-of-" noise falls back to single-file size.
    #[test]
    fn split_gguf_bytes_sum_all_shards() {
        let dir = std::env::temp_dir().join(format!("split-gguf-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap(); // unwrap: test fixture tempdir must exist
        let w = |name: &str, len: usize| {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; len]).unwrap(); // unwrap: test fixture shard write must succeed
            p
        };
        let s1 = w("m-00001-of-00003.gguf", 100);
        w("m-00002-of-00003.gguf", 200);
        w("m-00003-of-00003.gguf", 300);
        let single = w("solo.gguf", 42);
        w("unrelated-00001-of-00002.bin", 999);
        assert_eq!(total_gguf_bytes(&s1), 600, "split must sum every shard");
        assert_eq!(total_gguf_bytes(&single), 42, "single file is its own size");
        std::fs::remove_dir_all(&dir).ok();
    }
