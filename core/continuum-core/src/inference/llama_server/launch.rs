//! The launch path: `PreparedLocalLaunch` (the private recipe an exact restore replays),
//! `prepare_local_launch` (resolve the GGUF, genes, projector, train dir, KV decision,
//! backend receipt and flag spine into an invocation) and `launch_prepared` (transition,
//! kill the predecessor, reclaim the port, reserve the page dir, spawn, pump stderr,
//! register the lane, wait ready, verify placement, publish the page contract).

use super::*;

/// Private original launch recipe; no rediscovery at replay.
#[derive(Clone)]
pub(super) struct PreparedLocalLaunch {
    pub(super) target: ServingTarget,
    pub(super) engine_program: String,
    pub(super) endpoint: String,
    pub(super) invocation: crate::inference::lane_args::LaneInvocation,
    pub(super) engine_install: Option<crate::inference::engine_install::EngineInstallReceipt>,
    pub(super) gguf: PathBuf,
    pub(super) slot_save_dir: PathBuf,
    /// The KV type the engine RUNS, for the page contract: the plan's effective type, never
    /// the launcher flag (f16 is passed as no flag at all, which read as "unknown" and cost
    /// every CPU lane its saved-page ledger, IntelMac 2026-10-11). `None` = genuinely unknown.
    pub(super) page_cache_type: Option<String>,
    pub(super) mmproj: bool,
    // Err retains an ordinary/legacy launch without inventing restoration authority.
    pub(super) local_inputs: Result<crate::inference::launch_inputs::LocalLaunchInputs, String>,
    pub(super) managed_environment: Vec<(String, Option<std::ffi::OsString>)>,
    pub(super) expected_geometry: Option<(u32, u32)>,
}

pub(super) fn unsupported_launch_environment(managed: &[(String, Option<std::ffi::OsString>)]) -> bool {
    std::env::vars_os().any(|(key, _)| {
        let key = key.to_string_lossy().to_ascii_uppercase();
        (key.starts_with("LLAMA_") || key.starts_with("GGML_"))
            && !managed.iter().any(|(known, _)| known == &key)
    })
}

impl PreparedLocalLaunch {
    pub(super) fn verify_geometry(&self, context: u32, lanes: u32) -> Result<(), LlamaServerError> {
        if self
            .expected_geometry
            .is_some_and(|expected| expected != (context, lanes))
        {
            return Err(LlamaServerError::Spawn(
                "restored engine geometry differs from the original verified launch".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn command(&self) -> Result<tokio::process::Command, LlamaServerError> {
        let mut command = match &self.engine_install {
            Some(receipt) => receipt.command().map_err(LlamaServerError::Spawn)?,
            None => tokio::process::Command::new(&self.engine_program),
        };
        command.args(&self.invocation.args);
        for (key, value) in &self.managed_environment {
            match value {
                Some(value) => {
                    command.env(key, value);
                }
                None => {
                    command.env_remove(key);
                }
            }
        }
        for (key, value) in &self.invocation.envs {
            command.env(key, value);
        }
        Ok(command)
    }

    pub(super) async fn validate_restoration(
        &self,
        endpoint: &str,
        program: &str,
    ) -> Result<(), LlamaServerError> {
        if self.endpoint != endpoint || self.engine_program != program {
            return Err(LlamaServerError::Spawn(
                "prepared launch belongs to another endpoint".into(),
            ));
        }
        if self.expected_geometry.is_none() {
            return Err(LlamaServerError::Spawn(
                "original launch has no verified geometry".into(),
            ));
        }
        let inputs = self
            .local_inputs
            .as_ref()
            .map_err(|reason| LlamaServerError::Spawn(reason.clone()))?;
        if unsupported_launch_environment(&self.managed_environment) {
            return Err(LlamaServerError::Spawn(
                "unclassified engine environment prevents exact restoration".into(),
            ));
        }
        let application = self.engine_install.as_ref().ok_or_else(|| {
            LlamaServerError::Spawn("original application receipt is absent".into())
        })?;
        application
            .confirm()
            .await
            .map_err(LlamaServerError::Spawn)?;
        inputs.validate().await.map_err(LlamaServerError::Spawn)
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
        let bin = self.engine_bin()?;
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
        let slot_save_dir = kv_page_dir(
            &target.model.id,
            target.served_total_ctx() / target.parallel_lanes().max(1),
        );
        // In-engine training is an addition to the lane, never a condition of it: a
        // directory that cannot be made turns /train off on this launch, says so, and
        // the lane serves exactly as it would have.
        let train_dir = engine_train_dir().and_then(|dir| match std::fs::create_dir_all(&dir) {
            Ok(()) => Some(dir),
            Err(e) => {
                tracing::warn!(
                    dir = %dir.display(),
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
        let kv_plan = crate::cognition::kv_cache_plan::resolve();
        let kv_cache_type = kv_plan.launcher_cache_type().map(|s| s.to_string());
        let flash_attn = kv_plan.flash_attn;
        // THE RECEIPT. The chosen type, WHERE it came from, the backend it was decided
        // against, what a token of KV therefore costs, and the window the plan derived
        // from that cost — one line, so a half-size lane can never again be invisible.
        // `kv_per_token` comes through the plan's OWN transform (`footprint_for` →
        // `apply_kv_quantization`), which reads the same resolved divisor this launch
        // flags with: if these two ever disagreed, the box would over- or under-commit
        // its whole budget by 2×.
        let engine_advertised = crate::cognition::kv_cache_plan::engine_quantized_kv_support();
        let planned_kv_per_token = crate::cognition::serving_plan::footprint::footprint_for(&target.model)
            .map(|fp| fp.kv_per_token)
            .unwrap_or(0); // 0 = no footprint resolvable for this row; `answered` carries the absence
        crate::probe!(
            class = "serving.kv_cache.decided",
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
            MainLaneMmproj::Withhold => crate::probe!(
                class = "serving.vision.mmproj_withheld",
                model = %target.model.id,
                "mmproj resolved but WITHHELD from the main lane (model row: \
                 mmproj_on_main_lane=false — multimodal disables cache_reuse); a \
                 sidecar VL candidate exists on this node and sight routes through it"
            ),
            MainLaneMmproj::WearAsOnlyEyes => crate::probe!(
                class = "serving.vision.mmproj_on_main_lane_as_only_eyes",
                model = %target.model.id,
                "mmproj goes on the MAIN lane: the row prefers a sidecar but no other \
                 VL candidate is on this node — the only eyes here beat cache_reuse \
                 (multimodal disables it); pull a *-VL-* row to restore the split"
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
            tracing::warn!(
                probe_class = "serving.vision.no_mmproj",
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
                crate::probe!(
                    class = "serving.version_probe_timeout",
                    bin = %bin,
                    "`--version` did not answer in 10 s — neither verified nor refused; launching anyway"
                );
                String::new()
            }
        };
        crate::probe!(
            class = "serving.server_version",
            bin = %bin,
            version = %version.lines().next().unwrap_or("").trim(), // unwrap_or: no output = empty receipt, spawn reports the real failure
            "the serving binary named its build"
        );
        if is_debug_build(&version) {
            crate::probe!(
                class = "serving.debug_build_refused",
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
                crate::probe!(
                    class = "serving.backend_probe_failed",
                    bin = %bin,
                    error = %e,
                    "`--list-devices` could not run — the spawn below reports the real failure"
                );
                None
            }
            Err(_) => {
                crate::probe!(
                    class = "serving.backend_probe_timeout",
                    bin = %bin,
                    "`--list-devices` did not answer in 10 s — backend initialisation hangs on \
                     this host; the lane serves on the CPU and says so"
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
        crate::probe!(
            class = "serving.backend_receipt",
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
                crate::probe!(
                    class = "serving.backend_refused",
                    bin = %bin,
                    reason = %reason,
                    "refused to serve from a binary that loads no GPU backend on a GPU host"
                );
                return Err(LlamaServerError::Spawn(reason.clone()));
            }
            crate::inference::backend_receipt::BackendVerdict::ProbeHung => {
                // Last flag wins in llama-server's parser: pin the lane to the CPU.
                invocation.constrain_to_cpu();
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
            engine_program: bin.clone(),
            endpoint: self.root.clone(),
            invocation,
            engine_install,
            gguf,
            slot_save_dir,
            page_cache_type: Some(kv_plan.cache_type.clone()),
            mmproj: mmproj.is_some(),
            local_inputs,
            managed_environment,
            expected_geometry: None,
        })
    }

    pub(super) async fn launch_prepared(
        &self,
        prepared: Arc<PreparedLocalLaunch>,
        current: &ServingLifecycleCheck<'_>,
        restore_session: Option<&OwnedRestoreSession>,
        mut reservation: Option<crate::resources::LeaseGuard>,
    ) -> Result<(), LlamaServerError> {
        if prepared.endpoint != self.root {
            return Err(LlamaServerError::Spawn(
                "prepared launch belongs to another endpoint".into(),
            ));
        }
        let restore_generation = restore_session.map(|session| session.expected.lock().clone());
        let restoring = restore_generation.is_some();
        if restoring {
            prepared.validate_restoration(&self.root, &prepared.engine_program).await?;
        }
        let target = &prepared.target;
        let gguf = prepared.gguf.clone();
        let slot_save_dir = prepared.slot_save_dir.clone();
        let page_cache_type = prepared.page_cache_type.clone();
        let invocation = &prepared.invocation;
        let engine_install = prepared.engine_install.clone();
        let (_, port) = split_host_port(&self.root);
        let lanes = target.parallel_lanes();
        let total_ctx = target.served_total_ctx();
        if let Some(receipt) = &engine_install {
            receipt.confirm().await.map_err(LlamaServerError::Spawn)?;
        }
        if let Ok(inputs) = &prepared.local_inputs {
            inputs.validate().await.map_err(LlamaServerError::Spawn)?;
        }
        let endpoint = crate::inference::slots::directory().endpoint(&self.root);
        let transition = match restore_generation.as_ref() {
            Some(expected) => {
                endpoint
                    .transition_for_generation_if(expected, &|| {
                        // Check every physical precondition before the caller's admission
                        // callback commits intent/artifact state or closes readiness.
                        let empty = self.child.lock().unwrap().is_none(); // JUSTIFIED: poison means a prior panic mutating the owned child.
                        expected.has_exited() && empty && current()
                    })
                    .await
            }
            None => endpoint.transition_if(current).await,
        }
        .map_err(|()| LlamaServerError::Superseded)?;

        // One server at a time: kill the old child before binding the port.
        // Whether we already OWN a child decides if a stale-orphan reap is needed:
        // a relaunch (we own one) frees the port via `kill_child` and must NOT pay
        // the reclaim wait; a fresh claim (we own none) is where a crashed
        // predecessor's orphan may still hold the canonical port.
        let had_own_child = self.child.lock().unwrap().is_some();
        self.kill_child();

        // A freed listener is not proof that the predecessor stopped decoding.
        // Poll ONLY its retained owned Child; cancelled/failed transitions remain
        // closed, and the next attempt waits on the same generation's receipt.
        if let Some(previous) = transition.previous_generation() {
            let wait = async {
                let mut tick = tokio::time::interval(Duration::from_millis(100));
                loop {
                    tick.tick().await;
                    PAGE_DIR_GUARD.lock().collect_exited();
                    if previous.has_exited() {
                        break;
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(10), wait)
                .await
                .map_err(|_| {
                    LlamaServerError::Spawn(
                        "predecessor child exit remains unverified; endpoint stays suspended"
                            .into(),
                    )
                })?;
        }

        // Fresh claim on the live lane: reap a crashed predecessor's orphaned
        // llama-server if its pidfile still names one holding our port, so the
        // bind below succeeds instead of failing loud against it. This is reached
        // ONLY when reconcile decided NOT to adopt (wrong model/genome, or the
        // port is unreachable) — a HEALTHY orphan serving the right model is
        // adopted for free upstream (`ensure_model_serving` → `AlreadyServing`,
        // zero reload) and never reaches `serve`. The reap is identity-verified
        // (never a reused pid) and a no-op when there is nothing to reclaim; if a
        // FOREIGN squatter we can't reclaim holds the port, the spawn's bind fails
        // loud rather than fleeing to a GPU-competing port
        // ([[fallbacks-are-illegal-fail-loud]]).
        if self.is_live_lane && !had_own_child && !restoring {
            let outcome = crate::inference::lane_pidfile::reclaim(port).await;
            crate::probe!(
                class = "serving.lane_reclaim",
                port = port,
                outcome = format!("{outcome:?}").as_str(),
                "fresh-claim reclaim of canonical serving port before spawn",
            );
        }

        // KILL-VERIFY GATE (2026-07-23 ready-flap case): llama-server does NOT
        // retry a lost bind — spawned against a still-held port it exits
        // instantly and `wait_ready` burns its whole budget polling a corpse,
        // which is the flap that killed 3,318 live turns in one day. Verify the
        // port is actually free BEFORE spawning: a short grace absorbs a normal
        // teardown (a Metal-resident model takes seconds to release); if the
        // port is STILL held, name the holder — a verified llama-server (the
        // predecessor whose pidfile was disarmed, an adopted-then-churned
        // orphan) is reaped and re-verified; anything else fails loud now
        // instead of wasting the ready budget ([[fallbacks-are-illegal-fail-loud]]).
        if !crate::inference::lane_process::wait_port_free(port, Duration::from_secs(8)).await {
            if restoring {
                return Err(LlamaServerError::Spawn(
                    "restoration refuses an occupied endpoint; no unowned process is reaped".into(),
                ));
            }
            match crate::inference::lane_process::pid_listening_on_port(port) {
                Some(pid) if crate::inference::lane_process::is_llama_server(pid) => {
                    crate::probe!(
                        class = "serving.lane_kill_verify",
                        port = port,
                        holder_pid = pid,
                        "port still held after teardown grace — reaping the verified llama-server holder",
                    );
                    crate::inference::lane_process::kill9(pid);
                    if !crate::inference::lane_process::wait_port_free(
                        port,
                        Duration::from_secs(10),
                    )
                    .await
                    {
                        return Err(LlamaServerError::Spawn(format!(
                            "port {port} still held after verified reap of llama-server pid {pid} — refusing to spawn against a bound port"
                        )));
                    }
                }
                holder => {
                    return Err(LlamaServerError::Spawn(format!(
                        "port {port} held by {holder:?} (not a verifiable llama-server) — refusing a blind spawn that would flap ready; free the port or change the lane plan"
                    )));
                }
            }
        }

        self.mmproj_on_lane.store(
            if prepared.mmproj { 2 } else { 1 },
            std::sync::atomic::Ordering::Relaxed,
        );
        self.launched_prompt_cache_mib.store(
            target.host_prompt_cache_mib,
            std::sync::atomic::Ordering::Relaxed,
        );
        // KV disk-paging dir (restore economy): GEOMETRY-KEYED — model + the
        // per-slot window — so a saved page can never be restored into a slot
        // of a different size or model (llama-server would refuse or corrupt).
        // Stale sibling generations for the SAME model are swept at spawn: a
        // relaunch that changed geometry orphans its old pages, and the spawn
        // is the one place that knows the new truth. Tracked + eviction-decided
        // in system_resources (the no-new-cache-dir-without-eviction law).
        // Remember this geometry ACROSS RUNS: the next boot's plan serves it first, so
        // the pages under this dir are still restorable after a reboot.
        // The geometry is remembered when the launch SETTLES (serving_daemon: cooldown
        // 1 → 0 with the lane serving), never at spawn — a spawn that gets re-homed within
        // its cooldown must not become the next boot's ceiling.
        // The dirs OTHER live lanes page into: protected from the sweep below. The
        // inventory is the durable lane registry (every recorded llama-server still
        // alive), not a process-local "last spawn" — a core restart that ADOPTS a
        // warm server has no spawn memory of it (Astra's review of #4069). Sweeping
        // at spawn (the prior order) deleted the running server's pages on every
        // relaunch attempt — including attempts that never replaced it (2026-09-15:
        // `--c59171` swept at 13:35 while the 08:44 server was still writing into it;
        // 9 restores then failed "failed to open"). A live lane recorded WITHOUT a
        // page dir (pre-field record) makes the inventory incomplete → no sweep.
        // Reserve NOW — before the dir is even created, before the child exists,
        // before the record is written — so a sibling spawn's sweep (live or
        // ephemeral; they share no gate) sees this dir in the inventory from this
        // instant and can never delete it between mkdir and the reservation (Astra's
        // third review of #4069). Released after the record carries it.
        let (mut page_dir_reservation, page_dir_ready) =
            PageDirReservation::take_and_create(&slot_save_dir);
        // Capture the server's stderr to a per-port log file (#175). llama.cpp prints
        // its load banner AND — critically — the underlying ggml/Metal fault behind a
        // `{"code":500,"message":"Compute error"}` HTTP reply to stderr. The prior
        // `Stdio::null()` threw that root cause away, so a compute-error storm was
        // undiagnosable from outside the process (glass-boxed 2026-07-15: every request
        // 500'd in ~7ms and the WHY went to /dev/null). Fail-soft: if the log can't be
        // opened, fall back to null and say so — an unreadable log must never block
        // serving (same posture as the pidfile below).
        // [[never-blind-feedback-driven-iteration]] [[self-test-via-command-feedback-surface-never-blind]]
        // Capture the server's stderr (#175). llama.cpp prints its load banner AND the
        // underlying ggml/Metal fault behind a `{"code":500}` reply here; `Stdio::null()`
        // threw that root cause away.
        //
        // PIPED, not a raw file fd. Handing the child a file means the CHILD owns the write
        // and nothing can bound it: on 2026-08-05 a wedged slot printed `progress = 1.10`
        // at 1.2 GB/min for four hours and the log reached 172 GB, taking the machine to
        // zero bytes free. `code::child_log` owns the file instead and rotates at a few MB.
        // [[never-blind-feedback-driven-iteration]]
        let log_path = dirs::home_dir().map(|h| {
            h.join(".continuum")
                .join("logs")
                .join(format!("llama-server-{}.log", port))
        });
        if let Some(dir) = log_path.as_ref().and_then(|p| p.parent()) {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut cmd = prepared.command()?;
        // THE LANE IS ITS OWN PROCESS GROUP (card 59052747). A plain child sat in the
        // core's group, so every group-addressed signal aimed at the core — the stop
        // rail's `kill(-core_pid, SIGTERM)`, launchd reaping a job's group on exit —
        // took the warm engine with it, eleven lines before the reboot path promised to
        // leave it for adoption. Its fate is decided by NAME (the lane registry, the
        // ownership sweep, the adopt-or-reap rail), never by which group it happened
        // to be born into. Windows has no process groups here; ownership does the job.
        #[cfg(unix)]
        cmd.process_group(0);
        if let Some(receipt) = &engine_install {
            receipt.confirm().await.map_err(LlamaServerError::Spawn)?;
        }
        if let Ok(inputs) = &prepared.local_inputs {
            inputs.validate().await.map_err(LlamaServerError::Spawn)?;
        }
        // Clone before OS spawn, so allocation cannot strand an uninstalled child.
        let child_install = engine_install.clone();
        let child_inputs = prepared.local_inputs.as_ref().ok().cloned();
        // Admit the generation before creating a child: a refused predecessor
        // cannot leave a newly spawned process outside the retained owner.
        let generation = transition
            .start_generation()
            .map_err(LlamaServerError::Spawn)?;
        *self.last_started.lock() = Some(generation.clone());
        // No await separates generation creation, receipt advance and actual child
        // installation. A failed OS spawn marks this exact generation exited.
        if let Some(session) = restore_session {
            *session.expected.lock() = generation.clone();
        }
        let mut child = match cmd.stdout(Stdio::null()).stderr(Stdio::piped()).spawn() {
            Ok(child) => child,
            Err(error) => {
                generation.observed_exit(); // No child was created for this generation.
                return Err(LlamaServerError::Spawn(format!("{}: {error}", prepared.engine_program)));
            }
        };
        let child_pid = child.id();
        let child_stderr = child.stderr.take();
        if let Some(grant) = reservation.take() {
            *self.pending_resource_reservation.lock() = Some(grant);
        }
        self.install_child(child, generation.clone(), child_install, child_inputs);
        // Launch arguments are evidence of what we requested, not a readback of
        // cache capacity or occupancy. Adopted engines do not pass this seam.
        crate::probe!(
            class = "serving.prompt_cache.launch",
            model = target.model.id.as_str(),
            artifact = %gguf.display(),
            engine = %prepared.engine_program,
            pid = ?child_pid,
            port = port,
            requested_mib = target.host_prompt_cache_mib as u64,
            applied_mib = "unobserved",
            total_ctx = total_ctx as u64,
            lanes = lanes as u64,
            requested_cache_type_k = ?invocation.value_of("--cache-type-k"),
            requested_cache_type_v = ?invocation.value_of("--cache-type-v"),
            omitted_cache_type_source = "engine_default_unobserved",
            checkpoints = ?invocation.value_of("--ctx-checkpoints"),
            checkpoint_min_step = ?invocation.value_of("--checkpoint-min-step"),
            draft_model = ?invocation.value_of("--spec-draft-model"),
            serialized_state_bytes = "unobserved",
            rejection_count = "unobserved",
            "spawned engine with requested cache limit; applied limit and serialized-state telemetry are not exposed by current engine API"
        );
        // Hand stderr to the capped sink. If either the handle or the path is missing the
        // child still serves — unlogged, and the pipe drains to close so it cannot block.
        // The sink reads every line to keep the file capped, so it is also the cheapest
        // place to notice the engine reporting an IMPOSSIBLE state. A slot printing
        // `progress = 1.10` is wedged; the decode heartbeat can't see it (the OTHER slots
        // still decode, so the lane reads healthy) — which is how one slot burned four
        // hours on 2026-08-05. The watcher only RAISES; the serving daemon reaps.
        // The same pump also records the engine's OFFLOAD BANNER — the readback half of
        // the placement contract (#441): the governor planned a placement, the banner is
        // what the engine actually allocated, and serve() compares them after readiness.
        let offload_watch = Box::new(crate::inference::placement_watch::OffloadWatch::new(
            self.offload.clone(),
        ));
        let debug_flag = crate::inference::debug_build_watch::DebugBuildFlag::default();
        let watch: Box<dyn crate::inference::child_log::LineWatch> = match self.wedge.clone() {
            Some(flag) => Box::new(crate::inference::placement_watch::ChainWatch(
                Box::new(crate::inference::wedge::WedgeWatch::new(flag)),
                offload_watch,
            )),
            None => offload_watch,
        };

        let watch: Box<dyn crate::inference::child_log::LineWatch> =
            Box::new(crate::inference::placement_watch::ChainWatch(
                Box::new(crate::inference::debug_build_watch::DebugBuildWatch::new(
                    debug_flag.clone(),
                )),
                watch,
            ));
        match (child_stderr, log_path) {
            (Some(stderr), Some(path)) => crate::inference::child_log::drain_capped(stderr, path, watch),
            (Some(_), None) => tracing::warn!(
                probe_class = "serving.llama.stderr_unlogged",
                port = port,
                "no home dir for the llama-server log — serving unlogged (#175)"
            ),
            (None, _) => tracing::warn!(
                probe_class = "serving.llama.stderr_unlogged",
                port = port,
                "could not take llama-server stderr — serving unlogged (#175)"
            ),
        }

        // Record the child pid so a SIGKILLed core's successor can reclaim THIS
        // port instead of fleeing to a GPU-competing one. Live lane only — the
        // ephemeral lane owns its own scanned port and must not touch the canonical
        // pidfile. A write failure DISARMS future reclaim but must NOT fail the
        // serve: the server is up and serving; the pidfile is a recovery aid, not a
        // serving precondition. Surface it loud (probe) without aborting.
        if self.is_live_lane {
            match child_pid {
                Some(pid) => {
                    if let Err(e) = crate::inference::lane_pidfile::write(pid) {
                        crate::probe!(
                            class = "serving.lane_pidfile",
                            port = port,
                            error = e.to_string().as_str(),
                            "lane pidfile write failed — orphan reclaim disarmed for this run",
                        );
                    }
                }
                None => crate::probe!(
                    class = "serving.lane_pidfile",
                    port = port,
                    "spawned child has no pid — orphan reclaim disarmed for this run",
                ),
            }
        }

        // Register EVERY lane we spawn (live AND ephemeral) in the lane registry so
        // a crashed core's successor reaps this exact process instead of leaving a
        // ~6 GB orphan. `lane_pidfile` above only covers the live lane's canonical
        // port; the registry is what closes the ephemeral-lane leak. Same as the
        // pidfile, a write failure only DISARMS future reclaim for this lane — it
        // never fails the serve.
        if let Some(pid) = child_pid {
            let role = if self.is_live_lane {
                crate::inference::lane_registry::LaneRole::Live
            } else {
                crate::inference::lane_registry::LaneRole::Ephemeral
            };
            let rec = crate::inference::lane_registry::LaneRecord {
                pid,
                port,
                role,
                model: target.model_id().to_string(),
                // The SHAPE, so a successor core can size this lane as its own
                // without probing it — the difference between counting our
                // predecessor's bytes as ours and counting them as foreign.
                context_window: target.context_window,
                lanes: target.lanes,
                page_dir: Some(slot_save_dir.clone()),
                engine_bin: Some(PathBuf::from(&prepared.engine_program)),
                // the OS start time with the pid names this exact process (residency step 1)
                started_s: crate::inference::engine_residency::process_start_s(pid).unwrap_or(0), // unwrap_or: 0 never matches a live process, so an unreadable start time holds nothing
            };
            // Record + release under the page-dir guard (one critical section); a
            // failed write keeps the reservation for the lane's lifetime.
            if let Err(e) = page_dir_reservation.handoff(&rec) {
                crate::probe!(
                    class = "serving.lane_registry",
                    port = port,
                    error = e.to_string().as_str(),
                    "lane registry write failed — orphan reclaim disarmed for this lane",
                );
            }
        }

        // A reservation the handoff could not release (record write failed) lives
        // as long as the child does — beside it, not in this frame.
        *self.retained_page_dir.lock() = page_dir_reservation
            .is_held()
            .then_some(page_dir_reservation);
        // Remember the genome set this child was launched with — the truthful
        // catalog record for the relaunch decision (llama.cpp can't report it).
        *self.served_adapters.lock().unwrap() = target.adapter_paths();

        self.wait_ready().await?;
        // Second half of the debug-build gate (see debug_build_watch): the server
        // said so on its own stderr while coming up — refuse the lane, loudly.
        if debug_flag.is_set() {
            if let Some(pid) = child_pid {
                crate::inference::lane_process::kill9(pid);
            }
            crate::probe!(
                class = "serving.debug_build_refused",
                bin = %prepared.engine_program,
                "refused to serve from a DEBUG build (startup stderr) — build for speed"
            );
            return Err(LlamaServerError::Spawn(format!(
                "{} announced itself a DEBUG build on startup (asserts enabled; its speed is not valid). \
                 Rebuild llama-server in release from the canary pin and relaunch.",
                prepared.engine_program
            )));
        }
        // READY AND ACCEPTED: this lane owns its page dir. Sweep sibling generations
        // of the same model except every dir another live lane was launched with —
        // and only when the inventory is complete (Astra: "avoid sweeping when a live
        // predecessor's directory is unknown"). Then publish the dir for the
        // save-side trim — after the gates, and only if the dir exists (mkdir failed
        // = the paging tier is amputated for this serve; nothing to trim).
        // Census + sweep under ONE guard (Astra's re-review of #4069): no reservation
        // can be inserted or handed off between the census and the deletion.
        if !page_dir_ready {
            // No dir of our own (mkdir failed): the paging tier is amputated for this
            // serve — neither trim nor the destructive half runs (Cormac: symmetry).
        } else if sweep_stale_page_generations_guarded(&slot_save_dir).is_some() {
            // swept (possibly zero) with a complete inventory
        } else {
            crate::probe!(
                class = "inference.kv_page.sweep_skipped",
                reason = "a live lane's page dir is unknown (recorded before the field existed)",
                "stale-generation sweep skipped — the live-lane inventory is incomplete"
            );
        }
        if page_dir_ready {
            crate::inference::slots::note_page_dir(&slot_save_dir);
        }
        // PLACEMENT CONTRACT READBACK (#441, Joel 2026-08-15: "models that got fucked by
        // serving and are on cpu. You never catch it and we are waiting an eternity").
        // The governor planned a placement; the engine's own load banner says what it
        // ACTUALLY allocated. A GPU-placement lane that offloaded ZERO layers is the
        // whole model on CPU — it answers /health, decodes at a tenth of the planned
        // speed, and nothing else in the system can see the difference. This is a lease
        // violation, not a tuning note: probe LOUD so the daemon/operator respond,
        // never wait for a human to notice the eternity. (Partial offload is legit —
        // MoE cold-expert `-ot` splits offload a subset by design; only 0-of-N on a
        // GPU-intent lane is unambiguous.) The watcher only REPORTS; lifecycle stays
        // with the daemon, same doctrine as the wedge flag.
        if target.placement != LanePlacement::Cpu {
            // A GPU-intent lane that came up on the CPU is REFUSED, not reported.
            // Before 2026-09-05 this arm only probed; BigMama's 5090 then served
            // Devstral from system RAM behind `ready:true lanes:3` (the engine
            // said "no usable GPU found" on its own stderr, the pre-spawn device
            // probe had said CUDA0). "Ready" must be a verified observation — same
            // shape as the debug-build gate above: kill it, name the evidence, and
            // let the daemon's relaunch/degrade path own what happens next.
            // Joaquin's card 0c4317e1 (serve-time pin-match gap), finder credit.
            // THE CHANNEL FIRST (card 44ddb6fc, Joel: "stdout is never a transport"):
            // the engine's own `/props` says where the weights were ALLOCATED. An
            // older engine has no field — channel unavailable, said so once, and
            // the stderr arms below stay as the compensation until every tier
            // serves the fork.
            let residency = match self.weight_residency().await {
                Ok(Some(r)) => Some(r),
                Ok(None) => {
                    tracing::info!(
                        probe_class = "serving.placement.channel_unavailable",
                        port = port,
                        "engine reports no model_weight_buffers on /props — placement read \
                         from stderr only (fork 3ca60da3c carries the channel)"
                    );
                    None
                }
                Err(e) => {
                    tracing::info!(
                        probe_class = "serving.placement.channel_unreadable",
                        port = port,
                        error = %e,
                        "/props unreadable at readiness — placement read from stderr only"
                    );
                    None
                }
            };
            let channel_says_cpu = residency
                .as_ref()
                .is_some_and(|r| r.total_bytes() > 0 && r.accelerator_bytes() == 0);
            let declined = self.offload.gpu_declined();
            let zero_offload = matches!(self.offload.get(), Some((0, total)) if total > 0);
            if channel_says_cpu || declined || zero_offload {
                if let Some(pid) = child_pid {
                    crate::inference::lane_process::kill9(pid);
                }
                let residency_line = residency
                    .as_ref()
                    .map(|r| r.summary())
                    .unwrap_or_else(|| "channel unavailable".to_string()); // unwrap_or_else: no field = the channel could not be read; the label says so, never a placement
                let evidence = if channel_says_cpu {
                    "/props model_weight_buffers: 0 accelerator bytes"
                } else if declined {
                    "engine stderr: \"no usable GPU found\""
                } else {
                    "offload banner: 0 layers on the GPU"
                };
                crate::probe!(
                    class = "serving.placement.refused",
                    port = port,
                    model = target.model_id(),
                    evidence = evidence,
                    residency = residency_line.as_str(),
                    total_layers = self.offload.get().map(|(_, t)| t).unwrap_or(0), // unwrap_or: no banner = 0 layers KNOWN; the evidence field carries the declined line
                    "PLACEMENT VIOLATION: a GPU-placement lane came up ON CPU — refused, not served \
                     (ready must be a verified observation, #441 / card 0c4317e1)"
                );
                return Err(LlamaServerError::Spawn(format!(
                    "{} was planned on the GPU and came up on the CPU ({evidence}; residency {residency_line}); refused rather \
                     than serve every citizen from system RAM behind a green /health. Check the \
                     backend build (a DL-backend build can list CUDA0 from a shell and still fail \
                     to load it under the core) — last stderr:\n{}",
                    prepared.engine_program,
                    self.stderr_log_tail()
                )));
            }
            if let Some(r) = residency.as_ref() {
                crate::probe!(
                    class = "serving.placement.measured",
                    port = port,
                    model = target.model_id(),
                    accelerator_bytes = r.accelerator_bytes(),
                    total_bytes = r.total_bytes(),
                    residency = r.summary().as_str(),
                    "placement read from the engine's own channel — weights allocated as planned"
                );
            }
            match self.offload.get() {
                Some((0, _)) => unreachable!("zero offload on a GPU lane is refused above"),
                Some(_) => {}
                None => {
                    // No banner observed is a different fact from "reported 0" — an
                    // engine build that never prints it would otherwise read as
                    // healthy forever, which is exactly the silent class this exists
                    // to kill. Say so once, quietly.
                    tracing::info!(
                        probe_class = "serving.placement.unreported",
                        port = port,
                        "no offload banner observed by readiness — placement readback \
                         unavailable for this lane (#441)"
                    );
                }
            }
        }
        // GENOME DORMANCY (glass-boxed 2026-07-23): llama.cpp loads every
        // `--lora` at scale 1.0 — ALL ACTIVE, superimposed. Four personas'
        // adapters blended at full scale produced the degenerate-decode plague
        // (hangs, digit-splices, zero-walls, prompt-echo) the moment the
        // comma-fix made stacks REALLY load; the month of prior 'stability'
        // was the only-last-adapter bug accidentally serving one costume. The
        // design is paged activation: catalog LOADED, scales DORMANT (0.0),
        // per-request `lora` field activates. Zero them before ready — a lane
        // is not ready while wearing every costume at once.
        if !target.adapters.is_empty() {
            self.zero_adapter_scales().await;
        }
        let context = self.served_context_window().await?;
        let slots = self.served_lanes().await?;
        if context == 0 || slots == 0 || self.child_exit_status().is_some() {
            return Err(LlamaServerError::Spawn(
                "engine readiness lacks live slot geometry".into(),
            ));
        }
        if let Some(receipt) = &engine_install {
            receipt.confirm().await.map_err(LlamaServerError::Spawn)?;
        }
        if let Ok(inputs) = &prepared.local_inputs {
            inputs.validate().await.map_err(LlamaServerError::Spawn)?;
        }
        if restoring {
            prepared.verify_geometry(context, slots)?;
        }
        // Cold lifecycle metadata for paths already resolved by this launcher.
        // If any revision is unreadable (including a PATH-only engine name), do
        // not carry saved eligibility across engines. Never probe to rediscover it.
        let revisions = std::iter::once(gguf.clone())
            .chain(target.adapters.iter().map(|adapter| adapter.path.clone()))
            .chain(std::iter::once(PathBuf::from(&prepared.engine_program)))
            .map(|path| {
                let metadata = std::fs::metadata(&path).ok()?;
                Some((path, metadata.len(), metadata.modified().ok()?))
            })
            .collect::<Option<Vec<_>>>();
        transition
            .ready(
                &self.root,
                &generation,
                crate::inference::slots::KvPageContract {
                    model_id: target.model.id.clone(),
                    model: gguf,
                    adapters: target.adapter_paths(),
                    page_dir: page_dir_ready.then_some(slot_save_dir),
                    context,
                    slots,
                    cache_type: page_cache_type,
                    engine: prepared.engine_program.clone(),
                    revisions,
                },
            )
            .map_err(LlamaServerError::Spawn)?;
        // Name this process in its confirmed-save ledger, so a core that adopts this engine
        // after a deploy restores its saved pages instead of re-prefilling every mind.
        if let Some(pid) = child_pid {
            if let Some(started_s) = crate::inference::engine_residency::process_start_s(pid) {
                crate::inference::slots::stamp_engine_incarnation(&self.root, pid, started_s);
            }
        }
        self.record_verified_target(
            &generation,
            target,
            invocation,
            Some(prepared.clone()),
            &prepared.engine_program,
            context,
            slots,
        );
        Ok(())
    }

    pub(super) fn record_verified_target(
        &self,
        generation: &crate::inference::slots::EngineGeneration,
        target: &ServingTarget,
        invocation: &crate::inference::lane_args::LaneInvocation,
        prepared: Option<Arc<PreparedLocalLaunch>>,
        engine_program: &str,
        context: u32,
        lanes: u32,
    ) {
        let mut child = self.child.lock().unwrap(); // JUSTIFIED: poison means prior panic mutating the owned child.
        if let Some(owned) = child
            .as_mut()
            .filter(|owned| owned.generation.same_engine(generation))
        {
            let prepared = prepared.map(|original| {
                let mut verified = original.as_ref().clone();
                verified.expected_geometry = Some((context, lanes));
                Arc::new(verified)
            });
            owned.verified_target = Some(OwnedServingTarget {
                prepared,
                identity: OwnedEngineIdentity {
                    owner: Arc::downgrade(&self.child),
                    generation: generation.clone(),
                },
                target: target.clone(),
                engine_program: engine_program.to_string(),
                invocation: invocation.clone(),
                observed_context_window: context,
                observed_lanes: lanes,
                launched_host_prompt_cache_mib: self
                    .launched_prompt_cache_mib
                    .load(std::sync::atomic::Ordering::Relaxed),
            });
        }
    }

    /// The engine this launch runs: [`server_bin`], resolved now (see `prepare_local_launch`).
    pub(super) fn engine_bin(&self) -> Result<String, LlamaServerError> {
        #[cfg(test)]
        if let Some(pin) = &self.bin_pin {
            return Ok(pin.clone());
        }
        server_bin()
    }
}
