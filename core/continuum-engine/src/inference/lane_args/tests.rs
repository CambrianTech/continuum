#![cfg(test)]
    // what this catches (the 5090, 2026-10-11: launched --cache-ram 4288 against ~5.5 GB
    // measured states, 114 skips, 76% of restores cold, and nothing ever relaunched for it):
    // the agreed rule. A cache that holds no measured state is evidence for a re-home when a
    // relaunch would hold one; a cache that holds at least one state is never a reason,
    // however far below the derived target; nothing measured is never a reason; and a host
    // that cannot afford one state does not relaunch for nothing.
    #[test]
    fn a_lane_relaunches_for_cache_size_only_when_its_cache_holds_no_state() {
        let mib = 1024u64 * 1024;
        let state = Some(5_511 * mib);
        assert!(cache_relaunch_needed(4_288, 11_022, state), "the 5090: holds nothing, a relaunch holds two");
        assert!(!cache_relaunch_needed(6_000, 40_000, state), "holds one state: waits for the next relaunch, however far below target");
        assert!(!cache_relaunch_needed(4_288, 11_022, None), "nothing measured: no relaunch on a guess");
        assert!(!cache_relaunch_needed(4_288, 5_000, state), "a relaunch would hold no state either: no futile relaunch");
        assert!(!cache_relaunch_needed(0, 11_022, state), "an unrecorded launch size is not a measurement");
        assert!(launched_cache_holds_no_state(4_288, state) && !launched_cache_holds_no_state(5_512, state));
        // the adopt rail (card d7294f0b): the running grant comes off argv, unknown never relaunches
        assert!(!cache_relaunch_owed(None, 11_022, state), "no argv: unknown grant, no relaunch");
        assert!(cache_relaunch_owed(Some(3_248), 11_022, state), "Kimi's 3,248 MiB grant holds no 5.5 GB state");
        assert!(!cache_relaunch_owed(Some(3_248), 3_249, state), "a relaunch that would hold no state either is never owed");
    }

    // what this catches: sizing the host prompt cache per SLOT when the workload
    // is per CITIZEN. Measured 2026-08-28 — four citizens time-sharing one
    // `--parallel 1` slot ran a 0.29 prompt-cache hit rate, ~20k tokens
    // re-prefilled every generation, because 4 GiB held about one conversation
    // and every switch evicted somebody. This cache is a ~330x lever (0.1s
    // restore vs 32.9s re-prefill), so undersizing it does not cost a little —
    // it turns every context switch into recomputation. The hallmark is 14
    // personas collaborating; at that bar the constant is catastrophic.
    // what this catches: the 2026-09-21 M5 shape. A grant can be affordable and still be
    // structurally useless — below ONE conversation, prefix reuse is not "poor", it is
    // impossible, because the cache cannot retain a single prefix across a switch. pid
    // 33898 was spawned at 05:13:22Z with 480 MiB while deliberation prompts measured
    // 22,612-41,547 tokens at q8_0's 32 KiB/token, and every prefill that hour reported
    // cached=0. This pins the predicate the relaunch grow-check (card ab27b914) will use,
    // so the running-engine question and the spawn question can never drift apart.
    #[test]
    fn a_grant_below_one_conversation_cannot_hold_a_single_prefix() {
        let kv = 32 * 1024u64; // q8_0 on the 27B: 32,768 bytes/token, the measured value
        let mib = |bytes: u64| (bytes / (1024 * 1024)) as u32;

        // The real prompts that hour, largest first — one conversation is the LARGEST,
        // never the sum: it is the single prefix a switch must not evict.
        let measured = [22_612u32, 32_594, 37_213, 41_547];
        let one = super::one_conversation_bytes(&measured, kv);
        assert_eq!(one, kv * 41_547, "one conversation is the largest, not the total");
        assert_eq!(mib(one), 1_298, "≈1.3 GiB to hold a single 41.5k-token prefix");

        // The engine held 480 MiB. That is not a small cache, it is a broken one.
        assert!(480 < mib(one), "480 MiB cannot hold one conversation — reuse is structurally 0");

        // And the sizing function itself refuses to plan below one conversation when the
        // host can afford it: the floor is the max, even though only one citizen is big.
        let planned = super::host_prompt_cache_mib(&measured, kv, u64::MAX);
        assert!(planned >= mib(one), "the plan must hold at least one conversation: {planned}");

        // The trough that produced 480: affordability is a hard ceiling and CAN land under
        // one conversation. That is exactly when a relaunch must be armed once the ceiling
        // lifts — the engine cannot grow the grant it was born with.
        let squeezed = super::host_prompt_cache_mib(&measured, kv, 480 * 1024 * 1024);
        assert_eq!(squeezed, 480, "the affordability clamp still wins at spawn");
        assert!(squeezed < mib(one), "and the result is a cache that cannot hold one prefix");

        // Nothing measured = no conversation to hold, never a guessed size.
        assert_eq!(super::one_conversation_bytes(&[], kv), 0);
    }

    #[test]
    fn the_prompt_cache_is_sized_by_citizens_not_by_slots() {
        let kv = 32 * 1024u64; // ~32 KiB/token, Ornith-class geometry
        let plenty = u64::MAX;
        let mib = |bytes: u64| (bytes / (1024 * 1024)) as u32;

        // ONE conversation vs FOUR: the cache must grow with the citizens that
        // share the slot, because that is who evicts whom.
        let one = super::host_prompt_cache_mib(&[60_000], kv, plenty);
        let four = super::host_prompt_cache_mib(&[60_000; 4], kv, plenty);
        assert!(four > one, "four citizens need more than one: {four} vs {one}");
        assert_eq!(four, mib(kv * 60_000 * 4));

        // THE HALLMARK, and why max-window sizing was wrong. Fourteen MIXED
        // citizens (most chatting ~9k, three solving ~63k) must land in a
        // feasible envelope — sizing all fourteen at the 134k model window would
        // claim ~60 GiB and make the bar look impossible.
        let mut fourteen = vec![9_000u32; 11];
        fourteen.extend_from_slice(&[63_000, 63_000, 63_000]);
        let mixed = super::host_prompt_cache_mib(&fourteen, kv, plenty);
        let all_max = super::host_prompt_cache_mib(&[134_000; 14], kv, plenty);
        assert!(
            mixed < all_max / 2,
            "measured per-citizen demand must be far cheaper than N x max-window: \
             {mixed} MiB vs {all_max} MiB"
        );
        assert!(
            mixed < 16 * 1024,
            "fourteen mixed citizens must fit a sane envelope, got {mixed} MiB"
        );

        // THE GOVERNOR STILL WINS: affordability is a ceiling, never exceeded.
        let squeezed = super::host_prompt_cache_mib(&[60_000; 14], kv, 6 * 1024 * 1024 * 1024);
        assert_eq!(squeezed, 6144, "the lease's affordability clamps the want");

        // The floor is ONE conversation, DERIVED — never a model-specific
        // constant. Below it every switch is a guaranteed full re-prefill.
        let floor_only = super::host_prompt_cache_mib(&[63_000], kv, 1);
        assert_eq!(
            floor_only,
            super::CACHE_RAM_HARD_FLOOR_MIB,
            "fully starved we still pass a real number, never a zero"
        );
        assert!(
            super::host_prompt_cache_mib(&[], kv, plenty) >= super::CACHE_RAM_HARD_FLOOR_MIB,
            "no citizens yet is a cold start, not a zero-size cache"
        );
    }


    use super::*;
    use std::path::PathBuf;

    fn inv(lanes: u32, total_ctx: u32) -> LaneInvocation {
        base_invocation(
            &PathBuf::from("/models/m.gguf"),
            "qwen3-coder",
            "127.0.0.1",
            58057,
            lanes,
            total_ctx,
            CACHE_RAM_MIB, // the cold-start prior — these tests pin arg shape, not sizing
            &PathBuf::from("/tmp/kv-pages/test"),
        )
    }

    // what this catches: the KV disk-paging tier silently amputated. Slot
    // save/restore (`/slots/{id}?action=…`) only exists when the server was
    // LAUNCHED with `--slot-save-path`; a lane missing it turns every rotation
    // on an oversubscribed server into a full re-prefill (hit_rate 0.0 across
    // all acts, 2026-09-01) while the code above it believes paging works.
    // what this catches: a lane launched without context checkpoints dense enough
    // for a hybrid-cache model — the 2026-09-13 KV reuse 0.0 on every pinned turn.
    #[test]
    fn context_checkpoints_are_dense_enough_for_a_hybrid_cache() {
        let i = inv(5, 5 * 138_240);
        assert_eq!(i.value_of("--ctx-checkpoints"), Some("32"));
        assert_eq!(i.value_of("--checkpoint-min-step"), Some("4320"));
    }

    // regression for the 5090, 2026-10-05 (cached 15064 or 0 on Kimi's 55k turns):
    // 32 checkpoints at a fixed 1024 covered 32k, so a longer prompt erased the
    // oldest one, the stable head's, and the next turn re-prefilled from 0.
    // what this catches: a slot whose checkpoints stop short of its window, and a
    // short window losing the 1024 granularity.
    #[test]
    fn checkpoints_cover_the_whole_slot_window_at_the_finest_spacing_that_does() {
        for per_slot in [8_192u32, 32_768, 73_216, 138_240, 262_144] {
            let step = checkpoint_min_step(per_slot);
            assert!(step * CTX_CHECKPOINTS >= per_slot, "{per_slot}: {step} x {CTX_CHECKPOINTS} stops short");
            assert!(step >= MIN_CHECKPOINT_STEP);
        }
        assert_eq!(checkpoint_min_step(8_192), MIN_CHECKPOINT_STEP, "a short window keeps the kilotoken spacing");
        assert_eq!(checkpoint_min_step(73_216), 2_288);
        assert_eq!(inv(2, 2 * 73_216).value_of("--checkpoint-min-step"), Some("2288"));
    }

    // what this catches (card 912e606d, the MacBookPro 2026-10-21): a lane launched without
    // `--n-predict` lets a request with no max_tokens decode forever (n_predict -1 plus
    // context shift), holding its lane; the uncapped default is one per-lane window.
    #[test]
    fn an_uncapped_request_defaults_to_one_lane_window() {
        assert_eq!(inv(2, 2 * 32_768).value_of("--n-predict"), Some("32768"));
        assert_eq!(inv(1, 32_768).value_of("--n-predict"), Some("32768"));
        assert_eq!(inv(5, 5 * 138_240).value_of("--n-predict"), Some("138240"));
    }

    #[test]
    fn slot_save_path_is_part_of_the_unconditional_spine() {
        let i = inv(4, 65_536);
        assert_eq!(
            i.value_of("--slot-save-path"),
            Some("/tmp/kv-pages/test"),
            "the paging tier must be launched into existence: {:?}",
            i.args
        );
    }

    // what this catches: the window-quartering regression — `--parallel` inherited
    // llama.cpp's default of 4 while `-c` was passed as a per-lane window, so every
    // slot silently served a QUARTER of the planned context. Nothing failed; citizens
    // were just quietly given 1/4 of their window.
    //
    // This is the whole reason flags became data. The arithmetic was previously
    // welded into a `Command` chain, so asserting it meant launching a real
    // llama-server on a real GPU with a real model — which no unit test does, which
    // is why it went unnoticed. Now it is a pure function and this is three lines.
    #[test]
    fn parallel_is_explicit_and_total_ctx_is_the_per_lane_window_times_lanes() {
        let i = inv(2, 32_768);
        assert_eq!(
            i.value_of("--parallel"),
            Some("2"),
            "--parallel must be passed EXPLICITLY — inheriting llama.cpp's default of \
             4 is the bug that quartered every citizen's window\n{:?}",
            i.args
        );
        assert_eq!(
            i.value_of("-c"),
            Some("32768"),
            "-c is the TOTAL KV across slots (per-lane window x lanes), not the \
             per-lane window\n{:?}",
            i.args
        );
    }

    // what this catches: a "cheap" future edit re-enabling context shift, which does
    // not fail — it silently evicts the MIDDLE of an over-long prompt and generates
    // from the mutilation. #139: 44k-token prompts observed riding ~13.4k slots with
    // no error anywhere. Overflow must 400, so the caller's fail-loud path can report
    // the real defect (a RAG budget that overshot the window).
    #[test]
    fn context_shift_stays_disabled_on_every_lane() {
        assert!(
            inv(1, 4096).has("--no-context-shift"),
            "overflow must FAIL loudly, never amputate the prompt middle"
        );
    }

    // what this catches: dropping --cache-reuse while "cleaning up flags", which
    // costs a full re-prefill per turn and shows up only as latency. It was measured
    // at ~306s per act on a 36k conversation before the #266 ordering fix.
    #[test]
    fn cache_reuse_is_configured_for_prefix_reuse() {
        assert_eq!(inv(1, 4096).value_of("--cache-reuse"), Some("256"));
    }

    // what this catches: flag/value pairs drifting out of adjacency. Upstream turned
    // `--flash-attn` from a bare switch into one taking [on|off|auto] on 2026-08-20,
    // and the bare form then ATE THE NEXT ARGUMENT ("unknown value for --flash-attn:
    // '--mmproj'") — the lane failed to spawn and serving sat at 0. Any invocation
    // this module builds must be an even, well-formed sequence.
    #[test]
    fn every_flag_that_takes_a_value_is_followed_by_one() {
        let i = inv(4, 65_536);
        for flag in ["-m", "--alias", "--host", "--port", "-c", "--parallel", "--cache-reuse", "--ubatch-size"] {
            let v = i.value_of(flag);
            assert!(
                v.is_some_and(|v| !v.starts_with("--")),
                "{flag} must be followed by its VALUE, not by another flag \
                 (upstream's --flash-attn change is how this class breaks)\n{:?}",
                i.args
            );
        }
    }

    // what this catches: the base invocation quietly acquiring an env dependency.
    // Envs are spawn-time state that a caller must remember to apply; the conditional
    // one we DO have (LLAMA_RESIDENT_OVERRIDE, #29) is why `envs` exists on the
    // struct at all. If a base env ever appears, it must be a deliberate edit here
    // with its own justification, not a drive-by.
    #[test]
    fn the_base_invocation_needs_no_environment() {
        let base = inv(2, 16_384);
        assert!(base.envs.is_empty());
        assert!(base.inherited_env_keys.is_empty());
    }

    // what this catches: a conditional silently becoming unconditional (or vice versa).
    // Every flag below costs memory, changes numerics, or claims a capability, so
    // "present when it should be absent" is a real defect — the mmproj case literally
    // decides whether a citizen can SEE.
    #[test]
    fn no_options_means_no_conditional_flags() {
        let i = inv(2, 8192).with_options(&LaneOptions::default());
        for f in ["--cache-type-k", "--flash-attn", "--mmproj", "--spec-type"] {
            assert!(!i.has(f), "{f} must be absent with default options\n{:?}", i.args);
        }
        assert!(i.envs.is_empty(), "no options must mean no environment");
    }

    // what this catches: in-engine training silently off, or pointed somewhere other
    // than the governed directory. The fork refuses /train without --train-dir, so a
    // dropped flag sends every dream back to the retired trainer with nothing red; a
    // second or wrong value would let the route write outside the tracked dir.
    // what this catches: the prompt cache's disk tier dropped (an evicted conversation is then
    // re-prefilled whole) or launched without its limit (the engine would fill the volume).
    #[test]
    fn cache_disk_flags_ride_together_only_when_given() {
        let off = inv(1, 4096).with_options(&LaneOptions::default());
        assert!(!off.has("--cache-disk") && !off.has("--cache-disk-mib"), "{:?}", off.args);
        let dir = PathBuf::from("/home/u/.continuum/cache/kv-pages/prompt-cache/m");
        let on = inv(1, 4096).with_options(&LaneOptions {
            prompt_cache_disk: Some((&dir, 81_920)),
            ..LaneOptions::default()
        });
        assert_eq!(on.value_of("--cache-disk"), Some(dir.to_string_lossy().as_ref()));
        assert_eq!(on.value_of("--cache-disk-mib"), Some("81920"));
    }

    #[test]
    fn train_dir_flag_is_present_only_when_given_and_carries_that_path() {
        let off = inv(1, 4096).with_options(&LaneOptions::default());
        assert!(!off.has("--train-dir"), "no train dir must mean no flag\n{:?}", off.args);
        let dir = PathBuf::from("/home/u/.continuum/cache/engine-train");
        let on = inv(1, 4096).with_options(&LaneOptions {
            train_dir: Some(&dir),
            ..LaneOptions::default()
        });
        assert_eq!(on.value_of("--train-dir"), Some(dir.to_string_lossy().as_ref()));
        assert_eq!(on.args.iter().filter(|a| *a == "--train-dir").count(), 1);
        // A train-capable lane whose weights sit on the CPU serves them unrepacked, or
        // its learns fail (x86) or crash the engine (Metal host) — card 5b497852. A GPU
        // lane and a lane that cannot train keep repack's prompt speed.
        assert!(!on.has("--no-repack"), "GPU training lane keeps repack\n{:?}", on.args);
        let cpu_train = inv(1, 4096).with_options(&LaneOptions {
            train_dir: Some(&dir),
            cpu_only: true,
            ..LaneOptions::default()
        });
        assert!(cpu_train.has("--no-repack"), "{:?}", cpu_train.args);
        let paged_train = inv(1, 4096).with_options(&LaneOptions {
            train_dir: Some(&dir),
            expert_ot: Some("blk\\.3\\.ffn_.*_exps=CPU"),
            ..LaneOptions::default()
        });
        assert!(paged_train.has("--no-repack"), "{:?}", paged_train.args);
        let cpu_serve = inv(1, 4096).with_options(&LaneOptions {
            cpu_only: true,
            ..LaneOptions::default()
        });
        assert!(!cpu_serve.has("--no-repack"), "{:?}", cpu_serve.args);
    }

    // what this catches: GPU offload left to the backend's default. Metal defaults to
    // every layer, CUDA to none — so "absent" meant "all" on the Macs and "CPU" on
    // every CUDA host (2026-09-05, BigMama's 5090 idle at 674 MiB while a 24B model
    // ran on CPU and the lane wedged 13 times). The flag is present on every launch
    // and its value is the placement decision: all layers for GPU, 0 for CPU.
    #[test]
    fn gpu_offload_is_always_explicit_all_for_gpu_zero_for_cpu() {
        let gpu = inv(2, 8192).with_options(&LaneOptions::default());
        assert_eq!(gpu.value_of("--n-gpu-layers"), Some(ALL_GPU_LAYERS));
        let cpu = inv(2, 8192).with_options(&LaneOptions {
            cpu_only: true,
            ..LaneOptions::default()
        });
        assert_eq!(cpu.value_of("--n-gpu-layers"), Some("0"));
        assert_eq!(gpu.args.iter().filter(|a| *a == "--n-gpu-layers").count(), 1, "one placement, never a later override");
        // The MTP draft is placed the same way, never left to the engine's `auto` fit.
        let draft = std::path::Path::new("/m/mtp-x.gguf");
        let gpu_draft = inv(2, 8192).with_options(&LaneOptions { mtp_draft: Some(draft), ..LaneOptions::default() });
        assert_eq!(gpu_draft.value_of("--spec-draft-ngl"), Some(ALL_GPU_LAYERS));
        let cpu_draft = inv(2, 8192).with_options(&LaneOptions { mtp_draft: Some(draft), cpu_only: true, ..LaneOptions::default() });
        assert_eq!(cpu_draft.value_of("--spec-draft-ngl"), Some("0"));
    }

    // what this catches: a Metal lane launched without the GPU matrix units (opt-in in our
    // engine build), which ran the M5's prefill at about half speed.
    #[test]
    fn a_metal_lane_asks_for_the_matrix_units_and_an_operator_value_wins() {
        let mut lane = LaneInvocation::default();
        lane.use_metal_matrix_units(false);
        assert!(lane.envs.iter().any(|(k, v)| k == "GGML_METAL_TENSOR_ENABLE" && v == "1"));
        let mut operator = LaneInvocation::default();
        operator.use_metal_matrix_units(true);
        assert!(operator.envs.is_empty());
        assert_eq!(operator.inherited_env_keys, ["GGML_METAL_TENSOR_ENABLE"]);
    }

    // what this catches: THE 2026-08-20 SPAWN FAILURE, as a test instead of an outage.
    // Upstream turned --flash-attn from a switch into one taking [on|off|auto]; the bare
    // form ate the NEXT argument ("unknown value for --flash-attn: '--mmproj'") and the
    // lane refused to start, so serving sat at 0 lanes. The pairing below is the exact
    // adjacency that broke.
    #[test]
    fn flash_attn_carries_its_value_even_next_to_mmproj() {
        let mm = PathBuf::from("/models/mmproj.gguf");
        let i = inv(1, 4096).with_options(&LaneOptions {
            flash_attn: true,
            mmproj: Some(&mm),
            ..Default::default()
        });
        assert_eq!(i.value_of("--flash-attn"), Some("on"));
        assert_eq!(i.value_of("--mmproj"), Some("/models/mmproj.gguf"));
    }

    // what this catches: the resident override degrading into a command-line flag, or
    // being dropped entirely. It is an ENV var read by a loader hook (#29) — the reason
    // LaneInvocation models envs at all rather than returning a bare Vec<String>.
    #[test]
    fn resident_override_rides_the_environment_not_the_args() {
        let ro = PathBuf::from("/models/fit.gguf");
        let i = inv(1, 4096).with_options(&LaneOptions {
            resident_override: Some(&ro),
            ..Default::default()
        });
        assert_eq!(
            i.envs,
            vec![(
                "LLAMA_RESIDENT_OVERRIDE".to_string(),
                std::ffi::OsString::from("/models/fit.gguf")
            )]
        );
        assert!(
            !i.args.iter().any(|a| a.contains("fit.gguf")),
            "must not leak into args"
        );
        // Managed MoE defaults share the same environment owner. Inherited
        // operator settings are named but never overwritten or copied as values.
        let mut managed = i;
        let original_args = managed.args.clone();
        managed.inherit_or_default_env(
            "GGML_MOE_CAPTURE_FILE",
            Path::new("/trace/capture").as_os_str(),
            false,
        );
        managed.inherit_or_default_env(
            "GGML_MOE_PLAN_FILE",
            Path::new("/trace/unused-plan").as_os_str(),
            true,
        );
        managed.inherit_or_default_env(
            "GGML_MOE_TRACE_FILE",
            Path::new("/trace/events").as_os_str(),
            false,
        );
        assert_eq!(managed.args, original_args);
        assert_eq!(
            managed.envs.len(),
            3,
            "resident override plus two explicit defaults"
        );
        assert_eq!(
            managed.envs[1],
            (
                "GGML_MOE_CAPTURE_FILE".to_string(),
                std::ffi::OsString::from("/trace/capture")
            )
        );
        assert_eq!(
            managed.envs[2],
            (
                "GGML_MOE_TRACE_FILE".to_string(),
                std::ffi::OsString::from("/trace/events")
            )
        );
        assert_eq!(managed.inherited_env_keys, vec!["GGML_MOE_PLAN_FILE"]);
        assert!(managed
            .envs
            .iter()
            .all(|(key, _)| key != "GGML_MOE_PLAN_FILE"));
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let raw = std::ffi::OsString::from_vec(vec![b'/', 0xff]);
            let mut native = LaneInvocation::default();
            native.inherit_or_default_env("GGML_MOE_TRACE_FILE", &raw, false);
            assert_eq!(
                native.envs[0].1, raw,
                "native path bytes survive without lossy conversion"
            );
        }
    }

    // what this catches: KV quant emitting only one half of the pair. K and V are
    // separate flags; setting one and not the other is a silent asymmetry in the cache.
    #[test]
    fn kv_quant_sets_both_k_and_v() {
        let i = inv(1, 4096).with_options(&LaneOptions { kv_cache_type: Some("q8_0"), ..Default::default() });
        assert_eq!(i.value_of("--cache-type-k"), Some("q8_0"));
        assert_eq!(i.value_of("--cache-type-v"), Some("q8_0"));
    }

    // what this catches: the Flash-Next serving contract (measured 2026-08-28, the
    // first local serve). Three failure shapes, each hit live that night: (1) host
    // pins emitted as a SECOND --override-tensor beside the expert pager's — llama's
    // repeated-flag parsing risks last-wins (the --lora collapse shape), so both
    // MUST merge into one comma-joined value; (2) fit/warmup flags silently absent →
    // instant Metal OOM (fit counts the 35.8 GB pinned table as loadable) / a 36 GB
    // warmup fault-in; (3) max_ubatch RAISING the lane default instead of only
    // capping it (the default is sized to the resident lane's compute headroom).
    #[test]
    fn host_pins_merge_into_one_override_and_prefs_flags_land() {
        let i = inv(1, 4096).with_options(&LaneOptions {
            expert_ot: Some(r"blk\.(3|7)\.ffn.*_exps=CPU"),
            host_pinned_tensors: &["per_layer_token_embd.*"],
            fit_off: true,
            no_warmup: true,
            max_ubatch: Some(512),
            ..Default::default()
        });
        // ONE --override-tensor, comma-joined, pager first then pins.
        assert_eq!(i.args.iter().filter(|a| *a == "--override-tensor").count(), 1);
        assert_eq!(
            i.value_of("--override-tensor"),
            Some(r"blk\.(3|7)\.ffn.*_exps=CPU,per_layer_token_embd.*=CPU")
        );
        assert_eq!(i.value_of("-fit"), Some("off"));
        assert!(i.args.iter().any(|a| a == "--no-warmup"));
        assert_eq!(i.value_of("--ubatch-size"), Some("512"));

        // The cap only lowers: a ceiling above the lane default leaves it alone.
        let j = inv(1, 4096)
            .with_options(&LaneOptions { max_ubatch: Some(1 << 20), ..Default::default() });
        assert_eq!(j.value_of("--ubatch-size"), Some("2048"));
    }

