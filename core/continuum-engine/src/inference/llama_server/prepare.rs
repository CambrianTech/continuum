//! Prepare responsibility of the llama-server process owner.
use super::*;
impl LlamaServerProcess {
    /// The engine this launch runs, checked now (see `prepare_local_launch`): the slot holding
    /// this core's pin, as the artifact the spawn consumes ([`super::checked_engine`]). A refusal
    /// returns before anything launches, so no stale engine runs and no incumbent is touched.
    pub(super) fn checked_engine(&self) -> Result<super::checked_engine::CheckedEngine, LlamaServerError> {
        #[cfg(any(test, feature = "test-fixtures"))]
        if let Some(pin) = &self.bin_pin {
            return Ok(super::checked_engine::CheckedEngine::Operator { program: pin.clone() });
        }
        super::engine_pin::hold_current_engine_to_pin().map_err(LlamaServerError::EngineUnpinned)
    }
}

impl LlamaServerProcess {

    pub(super) async fn prepare_local_launch(
        &self,
        target: &ServingTarget,
    ) -> Result<PreparedLocalLaunch, LlamaServerError> {
        // The engine is resolved per launch, never cached on the process: after a promote the
        // next launch must take the slot `current` now names (card 2c5d0ec0), or #4464's
        // convergence relaunches onto the binary it is trying to leave.
        // The slot this core's source pins, never whichever one `current` was left naming: an
        // unpinned node refuses here, before the incumbent is touched (`EngineUnpinned`).
        let engine = self.checked_engine()?;
        // The slot's claim from the moment of validation: no build claims it while this holds.
        let engine_custody = engine.claim_slot().map_err(LlamaServerError::EngineUnpinned)?;
        let bin = engine.program().to_string();
        // Resolve the GGUF from the model struct already in hand — no re-fetch by
        // id. No file → fail loud; we never serve a substitute model
        // ([[fallbacks-are-illegal-fail-loud]]).
        let gguf: PathBuf = crate::model_registry::artifacts::resolve_gguf_for_model(&target.model)
            .ok_or_else(|| LlamaServerError::ModelNotFound(target.model.id.clone()))?;

        // Validate every gene file BEFORE killing the live server — a missing
        // `--lora` is a loud failure, and we must not tear down a healthy server
        // to launch one that will reject its own args. Fail loud, named cause.
        for adapter in &target.adapters {
            if !adapter.path.is_file() {
                return Err(LlamaServerError::AdapterNotFound(format!(
                    "{} ({})",
                    adapter.path.display(),
                    adapter.alias
                )));
            }
        }

        // Cold application hashing stays off runtime workers and before any
        // endpoint mutation. Legacy launches keep their existing loader behavior.
        let engine_install =
            crate::inference::engine_install::EngineInstallReceipt::prepare(bin.clone())
                .await
                .map_err(LlamaServerError::Spawn)?;
        let engine_command = || -> Result<tokio::process::Command, LlamaServerError> {
            match &engine_install {
                Some(receipt) => receipt.command().map_err(LlamaServerError::Spawn),
                None => Ok(tokio::process::Command::new(&bin)),
            }
        };

        let (host, port) = split_host_port(&self.root);

        // Resolve path inputs once while the original endpoint is untouched.
        // Receipted commands use a controlled cwd; relative paths are unsupported
        // there and must be refused before retiring the current generation.
        let resolved_mmproj =
            crate::model_registry::artifacts::resolve_mmproj_for_model(&target.model);
        let mtp_draft =
            crate::model_registry::artifacts::resolve_mtp_draft_for_model(&target.model);
        let slot_save_dir = kv_page_dir(&target.model.id);
        // In-engine training is an addition to the lane, never a condition of it: a
        // directory that cannot be made turns /train off on this launch, says so, and
        // the lane serves exactly as it would have.
        let train_dir = engine_train_dir().and_then(|dir| match std::fs::create_dir_all(&dir) {
            Ok(()) => Some(dir),
            Err(e) => {
                tracing::warn!(target: "continuum_core::inference::llama_server", dir = %dir.display(),
                    error = %e,
                    "engine train dir could not be created; /train is off on this lane"
                );
                None
            }
        });
        let moe_paths = if target.expert_placement.is_some() {
            moe_glass_box_paths(port)
        } else {
            None
        };
        if let Some(receipt) = &engine_install {
            receipt
                .require_absolute(&gguf)
                .map_err(LlamaServerError::Spawn)?;
            receipt
                .require_absolute(&slot_save_dir)
                .map_err(LlamaServerError::Spawn)?;
            if let Some(dir) = &train_dir {
                receipt
                    .require_absolute(dir)
                    .map_err(LlamaServerError::Spawn)?;
            }
            for path in target
                .adapters
                .iter()
                .map(|a| &a.path)
                .chain(target.resident_override.iter())
                .chain(resolved_mmproj.iter())
                .chain(mtp_draft.iter())
            {
                receipt
                    .require_absolute(path)
                    .map_err(LlamaServerError::Spawn)?;
            }
            if let Some(paths) = &moe_paths {
                for path in [&paths.capture, &paths.plan, &paths.trace] {
                    receipt
                        .require_absolute(path)
                        .map_err(LlamaServerError::Spawn)?;
                }
            }
        }

        // llama-server's `-c` is the TOTAL KV cache, split evenly across the
        // `--parallel` slots; each request only sees `-c / n_parallel` tokens.
        // The plan's `context_window` is PER-LANE, so the total we must request
        // is `context_window * lanes` — then each of `lanes` slots holds exactly
        // one planned window. We pass `--parallel` EXPLICITLY (never inherit
        // llama.cpp's default, which is 4 and silently quartered the window in
        // the prior bug). See `served_total_ctx` / `parallel_lanes`.
        let lanes = target.parallel_lanes();
        let total_ctx = target.served_total_ctx();
        // The unconditional flag spine moved to `lane_args` (decomposition slice 1,
        // docs/architecture/KV-CACHE-ECONOMY.md §7). It lives there because it is PURE
        // there — primitives in, an invocation out — and therefore assertable without
        // spawning a GPU process. From HERE it never could be, which is exactly how
        // `--parallel` silently quartered every citizen's window: the arithmetic was
        // welded into a Command chain no unit test could reach. Every flag's incident
        // comment moved WITH it; that comment is the only record of the failure that
        // set the value.
        // Resolution stays HERE (config_env reads, registry probes, the vision receipt);
        // `lane_args` decides only what a resolved value MEANS on a command line. That is
        // the split that keeps the flag surface pure and assertable — gather inputs, then
        // compute a plan, exactly as `TierPolicy` does.
        // Ask the ENGINE what KV cache types its build accepts, before deciding. Once
        // per process, bounded, with a named outcome — every probe on a launch path
        // gets both ([[every-probe-on-a-boot-or-launch-path-gets-a-bound-and-a-named-outcome]]).
        ensure_engine_kv_support_recorded(&bin, engine_command()?).await;
        // THE KV CACHE TYPE IS A DECISION, NOT A VARIABLE A HUMAN ONCE EXPORTED
        // (2026-09-20). This used to be two raw `config_env` reads: unset →
        // no `--cache-type-k/v` flag at all (the engine's f16 default, 65,536 B/token
        // for the 27B coder row) and `--flash-attn` off. The plan's resident-KV divisor
        // in serving_daemon read the SAME key, so the arithmetic was self-consistent —
        // it simply planned HALF the capacity, and nothing ever disagreed. The M5 had
        // the key set by hand; the 5090 never did (Joel measured a 26,880-token lane
        // there) and neither did the CPU-serving IntelMac (a 1.5B at -c 32768 on ~15 GB
        // usable, Cormac). Two of three boxes at half their KV budget, invisibly.
        //
        // Now: one resolved [`KvCachePlan`] — the engine's own advertised
        // `--cache-type-k` values where it gives them, the backend table otherwise —
        // and the plan's divisor comes off the SAME struct, so the flag and the fit
        // math cannot disagree. The env keys are an operator OVERRIDE, honored and
        // named in the receipt. (The CPU arm decides f16 pending a MEASUREMENT of the
        // dequant cost against that box's ~25 tok/s prefill — see the module header;
        // the override is how that number gets made.)
        let kv_plan = crate::inference::kv_cache_plan::resolve();
        let kv_cache_type = kv_plan.launcher_cache_type().map(|s| s.to_string());
        let flash_attn = kv_plan.flash_attn;
        // THE RECEIPT. The chosen type, WHERE it came from, the backend it was decided
        // against, what a token of KV therefore costs, and the window the plan derived
        // from that cost — one line, so a half-size lane can never again be invisible.
        // `kv_per_token` comes through the plan's OWN transform (`footprint_for` →
        // `apply_kv_quantization`), which reads the same resolved divisor this launch
        // flags with: if these two ever disagreed, the box would over- or under-commit
        // its whole budget by 2×.
        let engine_advertised = crate::inference::kv_cache_plan::engine_quantized_kv_support();
        let planned_kv_per_token = crate::inference::planning::serving_plan::footprint::footprint_for(&target.model)
            .map(|fp| fp.kv_per_token)
            .unwrap_or(0); // 0 = no footprint resolvable for this row; `answered` carries the absence
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.kv_cache.decided",
            model = %target.model.id,
            cache_type = %kv_plan.cache_type,
            source = kv_plan.source.as_str(),
            backend = kv_plan.backend.label(),
            engine_advertised = ?engine_advertised,
            flash_attn = flash_attn,
            divisor = kv_plan.bytes_per_token_divisor,
            kv_bytes_per_token = planned_kv_per_token,
            kv_rate_answered = planned_kv_per_token > 0,
            window = total_ctx as u64 / lanes.max(1) as u64,
            lanes = lanes as u64,
            total_ctx = total_ctx as u64,
            "the KV cache type this lane serves — decided from the backend (or an \
             operator override), and the window the plan sized from the same value"
        );
        // THE MAIN PERSONA LANE SERVES TEXT-ONLY — sight lives in the SIDECAR
        // (2026-08-24, the cache_reuse confession). llama-server hard-disables
        // `--cache-reuse` the moment an mmproj loads ("cache_reuse is not supported
        // by multimodal, it will be disabled" — in our own lane log at every boot),
        // and cache_reuse chunk-shifting is the mechanism the ENTIRE message-shaped
        // prompt economy leans on: working-memory evicts whole messages and the
        // conversation fitter drops whole old turns on the assumption the surviving
        // suffix realigns. With it silently off, every drop re-prefilled everything
        // after it — measured on round 437529ff: 6,419 of 30,004 tokens cached (the
        // system head only) with the slot EXCLUSIVELY the exam's. That is ~60-90s of
        // re-prefill tax per act, for every persona, always.
        //
        // Native ingestion on the main lane bought nothing that the designed
        // text-only path doesn't provide: `vision_lane_ready` reads /props (which
        // will now truthfully report no vision) and the SIDECAR VL lane +
        // description bridge take over — the exact flow every text-only mind
        // already uses. One sight path, not two. The mmproj artifact stays
        // resolved/fetched for the sidecar's use.
        // MODEL TRUTH, not blanket policy (Joel 2026-08-24: "we are not designing
        // around one model but many"): the row's `serving.mmproj_on_main_lane`
        // decides. Default FALSE (text-only main lane, cache_reuse alive); a
        // VL-first deployment opts in per model and pays the reuse cost knowingly.
        // THE ONLY EYES ON THE NODE GO ON THE MAIN LANE (2026-09-18, the 5090 after
        // its reset): Qwen3.8-27B's projector was on disk and resolved, this withhold
        // handed sight to "the sidecar", and the sidecar search — which rightly never
        // duplicates the main lane's weights — found no OTHER VL row on the box. Two
        // correct rules composed into a sightless node with its eyes in the cache,
        // and `vision_lane_ready` then said "--mmproj was passed" about a projector
        // that was withheld. So the withhold consults the same sidecar search FIRST:
        // another candidate exists → withhold as before (cache_reuse alive, sight via
        // the sidecar); none → the main lane wears the projector and pays the reuse
        // cost, named in the probe. A citizen is never blind because of a cache knob
        // (CLAUDE.md, Sensory Architecture).
        let decision = main_lane_mmproj_decision_for(&target.model, target.vision_sidecar);
        let mmproj: Option<std::path::PathBuf> = if decision.wears() {
            resolved_mmproj.clone()
        } else {
            None
        };
        match decision {
            MainLaneMmproj::Withhold => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.vision.mmproj_withheld",
                model = %target.model.id,
                "mmproj resolved but WITHHELD from the main lane (model row: \
                 mmproj_on_main_lane=false — multimodal disables cache_reuse); a \
                 sidecar VL candidate exists on this node and sight routes through it"
            ),
            MainLaneMmproj::WearAsOnlyEyes => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.vision.mmproj_on_main_lane_as_only_eyes",
                model = %target.model.id,
                "mmproj goes on the MAIN lane: the row prefers a sidecar but no other \
                 VL candidate is on this node — the only eyes here beat cache_reuse \
                 (multimodal disables it); pull a *-VL-* row to restore the split"
            ),
            MainLaneMmproj::WearReuseInert => continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.vision.mmproj_on_main_lane_reuse_inert",
                model = %target.model.id,
                "mmproj goes on the MAIN lane: this model's memory cannot shift \
                 (recurrent layers), so cache_reuse never runs on it and withholding \
                 the projector would only blind the node; prefix reuse and context \
                 checkpoints are unchanged"
            ),
            MainLaneMmproj::Wear | MainLaneMmproj::NoneResolved => {}
        }
        if resolved_mmproj.is_none() && target
            .model
            .capabilities
            .contains(&crate::model_registry::Capability::Vision)
        {
            // A Vision row with no projector anywhere is still a capability LIE —
            // the sidecar can't load what doesn't exist. Same receipt as before.
            tracing::warn!(target: "continuum_core::inference::llama_server", probe_class = "serving.vision.no_mmproj",
                model = %target.model.id,
                declared = ?target.model.mmproj_local_path,
                "vision-capable model has no resolvable mmproj projector — no sidecar sight \
                 possible. Fetch the mmproj GGUF (see the model row's mmproj_local_path) or drop \
                 the Vision capability so the row stops claiming sight."
            );
        }

        // Slice-3 resolution, same rule as above: look things up HERE, let `lane_args`
        // decide what they mean on a command line.
        let chat_template = gguf
            .parent()
            .map(|d| d.join("chat_template.jinja"))
            .filter(|p| p.is_file());
        let lora_paths: Vec<std::path::PathBuf> =
            target.adapters.iter().map(|a| a.path.clone()).collect();
        let expert_ot = target.expert_ot_value();

        let mut invocation = crate::inference::lane_args::base_invocation(
            &gguf,
            &target.model.id,
            &host,
            port,
            lanes,
            total_ctx,
            target.host_prompt_cache_mib,
            &slot_save_dir,
        )
        .with_options(&crate::inference::lane_args::LaneOptions {
            kv_cache_type: kv_cache_type.as_deref(),
            flash_attn,
            mmproj: mmproj.as_deref(),
            mtp_draft: mtp_draft.as_deref(),
            // Ngram spec is free (no tensors, no VRAM) and decode is half of
            // act latency — on for every GPU serving lane.
            // MEASURED OFF 2026-08-27: ngram spec ran at 2% draft acceptance on
            // live SWE-solve decode (1 of 48 draft tokens accepted, main lane,
            // 134k window) — at that rate the drafts cost more than they save
            // and tax every act. [[the-depth-decode-tax-is-our-lane-config]]:
            // the A/B said try ngram first; the A/B is done, evidence says off.
            // Revisit only with a measured acceptance ≥ ~30% on our real mix.
            ngram_spec: false,
            resident_override: target.resident_override.as_deref(),
            cpu_only: target.placement == LanePlacement::Cpu,
            chat_template: chat_template.as_deref(),
            loras: &lora_paths,
            // only to an engine that lists the flag: an older one refuses to start with it
            train_dir: train_dir.as_deref().filter(|_| engine_accepts_train_dir()),
            expert_ot: expert_ot.as_deref(),
            host_pinned_tensors: target.model.serving.host_pinned_tensors,
            fit_off: target.model.serving.fit_off,
            no_warmup: target.model.serving.no_warmup,
            max_ubatch: target.model.serving.max_ubatch,
            reasoning_budget: target.model.serving.reasoning_budget,
        });
        // MoE glass-box env seam (#278): when expert paging is active, the DAEMON
        // hands the fork its capture + plan file locations. Previously these envs
        // existed only when an operator hand-exported them before booting the
        // core, so every unattended MoE serve ran dark — no pager telemetry for
        // the serving console, no actuator channel for the policy controller.
        // The paths are the pure port derivation [`moe_glass_box_paths`]; the
        // positron serving source re-derives the same capture path from the
        // snapshot's port, so the two sides share one truth with no wire-type
        // change. An operator-exported env is an intentional campaign override
        // and is inherited untouched (config over convention — the child gets
        // the parent's env by default; we only fill the ABSENT case).
        if target.expert_placement.is_some() {
            if let Some(gb) = &moe_paths {
                invocation.inherit_or_default_env(
                    "GGML_MOE_CAPTURE_FILE",
                    gb.capture.as_os_str(),
                    std::env::var_os("GGML_MOE_CAPTURE_FILE").is_some(),
                );
                invocation.inherit_or_default_env(
                    "GGML_MOE_PLAN_FILE",
                    gb.plan.as_os_str(),
                    std::env::var_os("GGML_MOE_PLAN_FILE").is_some(),
                );
                invocation.inherit_or_default_env(
                    "GGML_MOE_TRACE_FILE",
                    gb.trace.as_os_str(),
                    std::env::var_os("GGML_MOE_TRACE_FILE").is_some(),
                );
            }
        }
        // BUILD FOR SPEED (Joel, 2026-09-04): a debug llama-server (asserts on)
        // serves numbers that are not valid and lanes that are several times too
        // slow — BigMama's 5090 hosted one for a day. Refuse it at the door, on
        // every machine, with the rebuild instruction; the version line is the
        // receipt either way.
        // BOUNDED: on IntelMac's box `llama-server --version` printed nothing,
        // went to state U and survived kill -9 (2026-09-05) — an unbounded await
        // here would have blocked every lane launch on that node forever. A
        // probe that times out neither verifies nor refuses; it says so and the
        // launch proceeds (the spawn below has its own failure shape).
        let version = match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            engine_command()?.arg("--version").output(),
        )
        .await
        {
            Ok(out) => out
                .map(|o| {
                    format!(
                        "{}{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    )
                })
                .unwrap_or_default(), // unwrap_or: a binary that cannot answer --version fails at spawn below with its own error
            Err(_) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.version_probe_timeout",
                    bin = %bin,
                    "`--version` did not answer in 10 s — neither verified nor refused; launching anyway"
                );
                String::new()
            }
        };
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.server_version",
            bin = %bin,
            version = %version.lines().next().unwrap_or("").trim(), // unwrap_or: no output = empty receipt, spawn reports the real failure
            "the serving binary named its build"
        );
        if is_debug_build(&version) {
            continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.debug_build_refused",
                bin = %bin,
                "refused to serve from a DEBUG build — build for speed"
            );
            return Err(LlamaServerError::Spawn(format!(
                "{} is a DEBUG build (asserts enabled; its speed is not valid). Rebuild llama-server in \
                 release from the canary pin and relaunch — a debug server never hosts a lane.",
                bin
            )));
        }
        // BACKEND RECEIPT (card c0bc4027): what does this binary actually LOAD?
        // 2026-09-05: BigMama's 5090 served a 24B model on the CPU because the
        // installed server could not load ggml-cuda.dll and fell back silently
        // (13 lane kills in 106 minutes before anyone read its log); IntelMac's
        // Intel-GPU Mac hangs in Metal initialisation. Ask `--list-devices`, bounded
        // like `--version` above: a GPU device → serve on it and name it; answered
        // with none → REFUSE (a warning on a CUDA host is the CPU-forever outage);
        // hung → CPU placement, said out loud. See inference/backend_receipt.rs.
        let devices_probe = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            engine_command()?.arg("--list-devices").output(),
        )
        .await;
        let receipt = match devices_probe {
            Ok(Ok(out)) => {
                let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
                text.push('\n');
                text.push_str(&String::from_utf8_lossy(&out.stderr));
                Some(crate::inference::backend_receipt::parse_list_devices(&text))
            }
            Ok(Err(e)) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.backend_probe_failed",
                    bin = %bin,
                    error = %e,
                    "`--list-devices` could not run — the spawn below reports the real failure"
                );
                None
            }
            Err(_) => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.backend_probe_timeout",
                    bin = %bin,
                    "`--list-devices` did not answer in 10 s — backend initialisation hangs on \
                     this host; the GPU lane is refused (never moved to the CPU) and retried"
                );
                None
            }
        };
        let cpu_by_plan = target.placement == LanePlacement::Cpu;
        let verdict = crate::inference::backend_receipt::backend_verdict(
            &bin,
            receipt.as_ref(),
            cpu_by_plan,
        );
        continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.backend_receipt",
            bin = %bin,
            backend = verdict.backend_label(),
            device = %match &verdict {
                crate::inference::backend_receipt::BackendVerdict::Gpu { device } => device.as_str(),
                _ => "",
            },
            load_errors = receipt.as_ref().map(|r| r.load_errors.len()).unwrap_or(0), // probe field: 0 = none or no transcript
            "the serving binary named the backend it loads"
        );
        match &verdict {
            crate::inference::backend_receipt::BackendVerdict::Refused { reason } => {
                continuum_base::probe!(target: "continuum_core::inference::llama_server", class = "serving.backend_refused",
                    bin = %bin,
                    reason = %reason,
                    "refused to serve from a binary that loads no GPU backend on a GPU host"
                );
                return Err(LlamaServerError::Spawn(reason.clone()));
            }
            // Measured M5 Pro 2026-10-19, same binary and 5,490-token prompt, arms
            // interleaved: prefill 1,163 / 1,153 tok/s without, 2,185 / 1,478 with.
            crate::inference::backend_receipt::BackendVerdict::Gpu { device } if device.starts_with("MTL") => {
                invocation.use_metal_matrix_units(std::env::var_os("GGML_METAL_TENSOR_ENABLE").is_some());
            }
            crate::inference::backend_receipt::BackendVerdict::Gpu { .. }
            | crate::inference::backend_receipt::BackendVerdict::CpuByPlan => {}
        }
        // Freeze managed values privately, including absence. The application
        // command may already have captured an inherited path before preparation.
        let command = engine_command()?;
        let managed_environment = [
            "LLAMA_RESIDENT_OVERRIDE",
            "GGML_MOE_CAPTURE_FILE",
            "GGML_MOE_PLAN_FILE",
            "GGML_MOE_TRACE_FILE",
            "LLAMA_TRACE",
            "GGML_METAL_TENSOR_ENABLE",
            "CUDA_VISIBLE_DEVICES",
            "CUDA_DEVICE_ORDER",
        ]
        .into_iter()
        .map(|key| {
            let mut value = std::env::var_os(key);
            for (name, setting) in command.as_std().get_envs() {
                if name == key {
                    value = setting.map(std::ffi::OsStr::to_os_string);
                }
            }
            for (name, setting) in &invocation.envs {
                if name == key {
                    value = Some(setting.clone());
                }
            }
            (key.to_string(), value)
        })
        .collect::<Vec<_>>();
        let unsupported_environment = unsupported_launch_environment(&managed_environment);
        let has_mutable_plan = managed_environment
            .iter()
            .any(|(key, value)| key == "GGML_MOE_PLAN_FILE" && value.is_some());
        let ambiguous_adapter = lora_paths
            .iter()
            .any(|p| p.to_str().is_none_or(|p| p.contains(',')));
        let local_inputs = if !cfg!(windows) || engine_install.is_none() {
            Err(
                "exact restoration requires original Windows application and local input pins"
                    .into(),
            )
        } else if target.expert_placement.is_some() || has_mutable_plan || unsupported_environment {
            Err("exact restoration lacks ownership for external engine configuration or expert actuator inputs".into())
        } else if ambiguous_adapter {
            Err("exact restoration cannot identify ambiguous adapter arguments".into())
        } else {
            let mut ggufs = vec![gguf.clone()];
            ggufs.extend(mmproj.iter().cloned());
            ggufs.extend(mtp_draft.iter().cloned());
            if let Some((_, Some(path))) = managed_environment
                .iter()
                .find(|(key, _)| key == "LLAMA_RESIDENT_OVERRIDE")
            {
                ggufs.push(PathBuf::from(path));
            }
            let mut other = lora_paths;
            other.extend(chat_template);
            crate::inference::launch_inputs::LocalLaunchInputs::capture(ggufs, other).await
        };
        Ok(PreparedLocalLaunch {
            target: target.clone(),
            engine,
            engine_custody: Arc::new(parking_lot::Mutex::new(engine_custody)),
            endpoint: self.root.clone(),
            invocation,
            engine_install,
            gguf,
            slot_save_dir,
            page_cache_type: Some(kv_plan.cache_type.clone()),
            page_flash_attn: Some(flash_attn),
            mtp_draft: mtp_draft.clone(),
            mmproj: mmproj.is_some(),
            local_inputs,
            managed_environment,
            expected_geometry: None,
        })
    }
}
