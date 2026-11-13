#![cfg(test)]
    use super::*;

    // what this catches: THE multi-shard correctness bug. A giant GGUF ships as N shards;
    // picking one and pulling only it leaves the model silently unloadable. expand_shard_set
    // must return EVERY shard of the chosen file's set — and must NOT false-positive a
    // single-file model (or a filename that merely contains "-of-" without a digit index).
    #[test]
    fn shard_set_expands_all_shards_but_leaves_single_files_alone() {
        let sharded = vec![
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00001-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00002-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00003-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00004-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00005-of-00006.gguf".to_string(),
            "UD-IQ1_M/GLM-5.2-UD-IQ1_M-00006-of-00006.gguf".to_string(),
            "UD-Q4_K_M/GLM-5.2-UD-Q4_K_M-00001-of-00011.gguf".to_string(), // a DIFFERENT quant set
        ];
        let set = expand_shard_set(&sharded[0], &sharded);
        assert_eq!(set.len(), 6, "all 6 IQ1 shards, and NOT the Q4 set");
        assert!(set.contains(&sharded[5]), "the last shard is included");
        assert!(
            !set.contains(&sharded[6]),
            "a different quant's shards are excluded"
        );

        // Single-file model → just itself.
        let single = vec!["qwen3-coder-compacted.Q4_K_M.gguf".to_string()];
        assert_eq!(expand_shard_set(&single[0], &single), single);

        // "-of-" with no digit index before it is NOT a shard (don't false-positive).
        let not_shard = vec!["model-proof-of-concept.gguf".to_string()];
        assert_eq!(expand_shard_set(&not_shard[0], &not_shard), not_shard);
    }

    // what this catches: the shard walker's extension is DERIVED, not hardcoded to `.gguf`.
    // Safetensors publish with the identical `-00001-of-000NN` convention, so a hardcoded
    // `.gguf` predicate silently reduced a 21-shard source checkout to ONE shard — the same
    // unloadable-model failure the test above pins for GGUF. Also pins that a shard set never
    // mixes extensions and never sweeps in a same-named `.index.json` sidecar.
    #[test]
    fn shard_set_walks_safetensors_and_never_mixes_extensions() {
        let files: Vec<String> = vec![
            "model-00001-of-00003.safetensors",
            "model-00002-of-00003.safetensors",
            "model-00003-of-00003.safetensors",
            "model-00001-of-00003.gguf", // a converted copy — a DIFFERENT artifact
            "model.safetensors.index.json",
            "config.json",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();

        let set = expand_shard_set(&files[0], &files);
        assert_eq!(set.len(), 3, "all 3 safetensors shards: {set:?}");
        assert!(
            set.iter().all(|f| f.ends_with(".safetensors")),
            "a shard set never mixes formats: {set:?}"
        );

        // The GGUF entrypoint in the SAME listing still resolves to only GGUF shards.
        let gguf_set = expand_shard_set(&files[3], &files);
        assert_eq!(gguf_set, vec!["model-00001-of-00003.gguf".to_string()]);
    }

    // what this catches: `auto` must resolve exactly the way it did before the `format`
    // parameter existed — GGUF whenever the repo publishes one. If this regresses, every
    // existing pull silently starts fetching unquantized source weights instead (hundreds of
    // GB, and not servable). Also pins that an explicitly requested format the repo lacks
    // fails loud NAMING what is there, instead of falling back to the other format.
    #[test]
    fn format_auto_prefers_gguf_and_explicit_misses_fail_loud() {
        let both: Vec<String> = vec!["m-Q4_K_M.gguf", "model.safetensors", "config.json"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let st_only: Vec<String> = vec!["model.safetensors", "config.json"]
            .into_iter()
            .map(str::to_string)
            .collect();

        assert_eq!(PullFormat::resolve(None, &both).unwrap(), PullFormat::Gguf);
        assert_eq!(
            PullFormat::resolve(Some("auto"), &both).unwrap(),
            PullFormat::Gguf
        );
        // Only safetensors published → auto reaches it rather than failing, which is the
        // whole point: a source-only repo used to be unacquirable through the command at all.
        assert_eq!(
            PullFormat::resolve(None, &st_only).unwrap(),
            PullFormat::Safetensors
        );
        assert_eq!(
            PullFormat::resolve(Some("safetensors"), &both).unwrap(),
            PullFormat::Safetensors
        );

        // Asked for GGUF, repo has none → refuse, and SAY what the repo actually has.
        let err = PullFormat::resolve(Some("gguf"), &st_only).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("safetensors"),
            "names what IS available: {msg}"
        );

        // An unknown format is a caller error, not a silent default.
        assert!(PullFormat::resolve(Some("onnx"), &both).is_err());

        // A repo with neither is a hard miss.
        assert!(PullFormat::resolve(None, &["README.md".to_string()]).is_err());
    }

    // what this catches: a source checkout is only loadable with its config/tokenizer/index
    // sidecars, and pulling "everything that isn't a weight" would drag in READMEs, images,
    // and frequently a second full copy of the weights — turning a 60 GB acquisition into
    // 200 GB. Pins that the companion set is exactly the loadable-checkout files.
    #[test]
    fn source_companions_take_the_loadable_set_and_nothing_else() {
        let files: Vec<String> = vec![
            "model-00001-of-00002.safetensors",
            "model.safetensors.index.json",
            "config.json",
            "tokenizer.json",
            "tokenizer_config.json",
            "chat_template.jinja",
            "README.md",
            "preview.png",
            ".gitattributes",
            "original/consolidated.00.pth",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();

        let got = source_companions(&files);
        assert_eq!(
            got,
            vec![
                "chat_template.jinja".to_string(),
                "config.json".to_string(),
                "model.safetensors.index.json".to_string(),
                "tokenizer.json".to_string(),
                "tokenizer_config.json".to_string(),
            ],
            "exactly the loadable-checkout sidecars"
        );
        assert!(
            !got.iter().any(|f| f.ends_with(".safetensors")),
            "weights come from the shard walker, never from the companion set"
        );
    }

    // what this catches: `pick_safetensors` must return shard 1 so the shard walker can
    // recover the set, and must skip a `consolidated` duplicate copy — a repo shipping both
    // layouts would otherwise be pulled twice.
    #[test]
    fn safetensors_pick_is_shard_one_and_skips_consolidated_copies() {
        let files: Vec<String> = vec![
            "model-00002-of-00002.safetensors",
            "model-00001-of-00002.safetensors",
            "consolidated/consolidated.safetensors",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(
            pick_safetensors(&files).unwrap(),
            "model-00001-of-00002.safetensors"
        );

        assert!(
            pick_safetensors(&["config.json".to_string()]).is_err(),
            "no weights → fail loud, never return a config as the model"
        );
    }

    // what this catches: the hint parser yields the owner/repo id hf-hub needs
    // for a HuggingFace hint, strips a `:tag`, and returns None (⇒ caller fails
    // loud) for a non-HF scheme like docker.io — we never feed a bogus id to the
    // download API.
    #[test]
    fn hint_parses_hf_and_rejects_non_hf() {
        assert_eq!(
            hf_repo_id("huggingface.co/bartowski/Qwen2-VL-7B-Instruct-GGUF"),
            Some("bartowski/Qwen2-VL-7B-Instruct-GGUF".to_string())
        );
        assert_eq!(
            hf_repo_id("huggingface.co/mlx-community/qwen2.5-7b-instruct-4bit"),
            Some("mlx-community/qwen2.5-7b-instruct-4bit".to_string())
        );
        assert!(
            hf_repo_id("docker.io/ai/qwen2.5:7B-Q4_K_M").is_none(),
            "a docker hint is not HF-acquirable — must be rejected, not guessed"
        );
    }

    // what this catches: quant selection honors an explicit request, never picks
    // a projector as the main file, prefers Q4_K_M by default, and fails loud
    // (not silent-substitute) when a requested quant is absent.
    #[test]
    fn quant_selection_is_explicit_and_loud() {
        let files = vec![
            "Qwen2-VL-7B-Instruct-Q4_K_M.gguf".to_string(),
            "Qwen2-VL-7B-Instruct-Q8_0.gguf".to_string(),
            "mmproj-Qwen2-VL-7B-Instruct-f16.gguf".to_string(),
            "README.md".to_string(),
        ];
        // Default ⇒ Q4_K_M (the balanced default), never the projector.
        assert_eq!(
            pick_gguf(&files, None).unwrap(),
            "Qwen2-VL-7B-Instruct-Q4_K_M.gguf"
        );
        // Explicit request honored.
        assert_eq!(
            pick_gguf(&files, Some("q8_0")).unwrap(),
            "Qwen2-VL-7B-Instruct-Q8_0.gguf"
        );
        // Absent requested tier ⇒ fail loud, never substitute.
        assert!(matches!(
            pick_gguf(&files, Some("q2_k")),
            Err(CommandError::NotFound(_))
        ));
        // The projector is picked separately.
        assert_eq!(
            pick_mmproj(&files).unwrap(),
            "mmproj-Qwen2-VL-7B-Instruct-f16.gguf"
        );
    }

    // what this catches: a repo with no model GGUF fails loud rather than
    // returning a non-gguf file or panicking on an empty list.
    #[test]
    fn no_gguf_fails_loud() {
        let files = vec!["README.md".to_string(), "config.json".to_string()];
        assert!(matches!(
            pick_gguf(&files, None),
            Err(CommandError::NotFound(_))
        ));
        assert!(pick_mmproj(&files).is_none());
    }

    #[test]
    fn name_mirrors_path() {
        use crate::sdk_codegen::ActionCommand;
        assert_eq!(ModelsPull::NAME, "models/pull");
    }
