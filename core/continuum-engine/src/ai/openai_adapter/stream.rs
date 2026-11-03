//! The streaming generation path: request assembly, SSE consumption, response shaping.

use super::*;
// The SSE wire types + stream consumer live in `crate::inference::sse_stream` (S3b decompose).
use crate::inference::sse_stream::warn_if_decode_collapsed;

impl OpenAICompatibleAdapter {
    pub(super) async fn stream_checked(
        &self,
        request: TextGenerationRequest,
        sink: tokio::sync::mpsc::UnboundedSender<GenerationChunk>,
    ) -> Result<TextGenerationResponse, crate::ai::inference_error::InferenceError> {
        // Only require API key for providers that need auth
        if self.config.requires_auth && self.api_key.is_none() {
            return Err(format!("{} not initialized", self.config.name).into());
        }

        let start = Instant::now();
        let endpoints = self.endpoints();
        let request_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| format!("req-{}", continuum_base::time::EpochMs::now().millis()));
        let raw_model = request
            .model
            .as_deref()
            .unwrap_or(self.config.default_model.as_str());

        // For DMR: resolve the logical model name to the actual model ID
        // stored in Docker Model Runner (which may have hf.co/ prefix and
        // different casing). Persona says "continuum-ai/qwen3.5-4b-code-forged-GGUF",
        // DMR has "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest".
        // If DMR doesn't have the model, resolve returns Err — we propagate
        // it as a fast, explicit failure instead of POSTing an unresolved
        // name and stalling on the 120s request timeout.
        let resolved_model: String = if self.config.dynamic_model_catalog {
            self.resolve_dmr_model_name(raw_model).await?
        } else {
            raw_model.to_string()
        };
        let model: &str = &resolved_model;

        // Native vision is a MODEL fact, not a provider fact: gate image content
        // parts on the TARGET model row's Capability::Vision (the same
        // `sensory::route` verdict that drives the bridge-vs-native table in
        // CLAUDE.md "Sensory Architecture"). Row present → its capability set is
        // the truth (a vision-capable llama-server lane / gpt-4o gets raw
        // pixels; a text row gets its images dropped and reads the description
        // bridge). Row absent (dynamic catalogs like DMR resolve ids the
        // registry never saw) → the provider-level scan ("any row under this
        // provider declares Vision", already folded into `config.capabilities`)
        // is the best available truth — same source `capabilities()` advertises.
        let vision_native = crate::model_registry::try_global()
            .and_then(|reg| {
                reg.model(raw_model).map(|row| {
                    crate::sensory::route(row, crate::sensory::Modality::ImageIn).is_native()
                })
            })
            .unwrap_or_else(|| self.config.capabilities.contains(&Capability::Vision));

        // Build the base request body — `inference::request_body` (the head of assembly).
        let audio_native = crate::model_registry::try_global()
            .and_then(|reg| reg.model(raw_model).map(|row| row.has(Capability::AudioInput)))
            .unwrap_or_else(|| self.config.capabilities.contains(&Capability::AudioInput));
        let mut body = crate::inference::request_body::build_base_body(
            &self.config,
            &request,
            model,
            vision_native,
            audio_native,
        );

        // Message content is final here; later extensions only add transport fields.
        // Carry this estimate through admission, overflow checks and send receipts.
        let prompt_tokens = crate::inference::serving_guard::approx_prompt_tokens(&body);

        // Held for the WHOLE turn: the concurrency permit + slot pin, as one RAII
        // guard bound HERE at function scope so it lives across the generation below
        // and releases both on drop. Assigned by turn admission inside the block (or,
        // for a non-llamacpp provider, by the permit-only fallback after it). This is
        // the scope that made the old inline permit/pin fragile — now it's one value.
        let mut _admission: Option<crate::inference::turn_admission::TurnAdmission> = None;
        // The pinned slot, named for the act deadline's per-request progress check (card
        // 6f3218ed); cleared when this generation ends.
        let mut _in_flight: Option<crate::inference::llama_server::InFlightSlotGuard> = None;
        if self.config.llamacpp_sampling_extensions {
            if let Some(obj) = body.as_object_mut() {
                apply_llamacpp_sampling_knobs(obj, &request);
                // KV-cache prefix reuse — the llama.cpp-server `cache_prompt`
                // extension. Without it the server re-prefills the ENTIRE prompt
                // from scratch every call. Our system prompt is a long, mostly
                // static prefix (identity + doctrine + tool catalog + roster);
                // re-prefilling it costs ~26-36s on the 14B (~216 tok/s prefill).
                // Measured 2026-06-23, Mac Metal: cache_prompt=false → 36s EVERY
                // call; cache_prompt=true → 36s cold then 0.48s warm (75×), because
                // warm calls prefill only the NEW tail tokens and reuse the matched
                // prefix from the slot's KV cache. The win is only as large as the
                // STABLE prefix — see render_assembled_context_within, which orders
                // static grounding before volatile recall so the cached prefix is
                // maximal. Same typed capability gate as repeat_penalty/lora: cloud
                // OpenAI-compat providers reject the non-standard field.
                obj.insert("cache_prompt".to_string(), json!(true));
                // PREFILL VISIBILITY — the llama.cpp `return_progress` extension.
                // MEASURED 2026-08-13 on the live lane: with this field absent
                // (its server-side default), a ~20k-token prompt produced ZERO
                // bytes for 286 SECONDS — the server says nothing at all between
                // accepting the request and emitting the first token. Both stream
                // watchdogs below budget 90s, so any prompt whose prefill exceeds
                // that was killed by US mid-ingest, and the server log shows
                // exactly that: `progress = 0.73 … 145 tok/s` immediately followed
                // by `srv stop: cancel task`. The retry then re-prefills from
                // scratch (the cancel also evicts the slot's prompt cache), so the
                // turn could never succeed — a citizen at full window occupancy was
                // structurally incapable of producing a token.
                //
                // With the field set, the slot emits a `prompt_progress` frame per
                // batch iteration (server-context.cpp:3703) — bytes AND a rising
                // `processed` counter, which is what makes the watchdog able to
                // tell healthy prefill from the #385 wedge instead of failing both.
                // Same typed capability gate as cache_prompt/repeat_penalty: cloud
                // OpenAI-compat providers reject non-standard fields.
                obj.insert("return_progress".to_string(), json!(true));
            }
            // Slot pinning (`id_slot`, llama.cpp extension): the warm KV in a slot
            // belongs to an ACTIVITY, not a persona. Rooms are 1:1 with activities in
            // continuum/airc, so the warm-context identity is (persona, room) — a
            // persona running N concurrent activities (e.g. N detached benchmark
            // solves, each in its own room) has N independently-growing prefixes and
            // needs N slots. Keying the lease on the bare persona collapsed all N onto
            // ONE slot, and each activity's turn clobbered the others' warm tail —
            // measured 2026-08-26 as cached:0 across EVERY turn of a 4-instance
            // dispatch even though the pin (persona→slot 0) was landing correctly.
            // The server reuses partial prefixes fine (proven same day: an 800-token
            // shared prefix reused, only the divergent tail re-prefilled), so the
            // defect was never the prompt or the server — it was N activities sharing
            // one slot. Non-persona traffic (evals, probes) stays unpinned so it can't
            // evict a citizen's warm slot.
            // TRAFFIC CLASS decides placement (slots::class_for over `purpose` —
            // one data map, no per-callsite hacks): only a TURN may hold or evict
            // a citizen's activity slot; every other class lands on the reserved
            // SCRATCH slot so it structurally cannot truncate a warm tail. The
            // measured defect: sidecar gate calls pinned the turn's own slot and
            // cut its ~30k tail to their common head — reuse broke even solo.
            let class = crate::inference::slots::class_for(request.purpose.as_deref());
            // MEASURED WORK HOLDS THE CORE (restore-economy Phase 1.a). Deferral
            // sits HERE — after classification, BEFORE the concurrency permit
            // below — so a parked Background/Probe request holds NOTHING while it
            // waits: no lock, no decode permit, just its own task on a notify.
            // Parking after the permit would be priority inversion (a sleeping
            // dream holding the only decode slot while a real turn queues behind
            // it — the exact too-serial hazard Joel flagged). Turn/Sidecar never
            // enter the await at all, and release wakes ALL waiters (broadcast,
            // not FIFO), so deferred work re-races admission rather than
            // draining serially. Measured cause: dream-belief-review took 52 of
            // 109 generations DURING a held solve, ~32.9s re-prefill per clobber.
            let hold_caller = request
                .persona_id
                .as_deref()
                .and_then(|p| uuid::Uuid::parse_str(p).ok());
            // FAIL-FAST, never wait (2026-08-29, the two-defer race): a caller
            // reaching this seam may already HOLD outer admission permits (the
            // serving lane, the prefill slot) acquired before a hold existed —
            // glass-boxed: a background gen passed the faculty's pre-gate defer,
            // took the lane, a solve then acquired the hold, and this seam
            // parked it WITH the lane in hand; every solve tick starved behind
            // it at lane_wait. Waiting here while holding permits is the
            // priority inversion itself. So this backstop REFUSES instead: the
            // error unwinds the caller's scope (permits drop), the settle
            // loop's deliberation-retry re-enters through the pre-gate defer,
            // which waits holding NOTHING. Bounded, inversion-free, race-closed.
            if crate::inference::measured_hold::should_defer(
                class,
                crate::inference::measured_hold::current().as_ref(),
                hold_caller,
            ) {
                continuum_base::probe!(
                    class = "inference.hold.backoff",
                    traffic = %class.as_str(),
                    purpose = %request.purpose.as_deref().unwrap_or("-"),
                    "hold active at the adapter seam — refusing fast so the caller drops its permits and retries through the pre-gate defer"
                );
                return Err(
                    "hold-backoff: measured work owns the core; generation refused so admission                      permits release — retry after the hold (automatic via deliberation retry)"
                        .into(),
                );
            }
            // Estimate this activity's prompt size once — the eviction price basis
            // AND the overshoot-alarm input below (chars/4, deliberately conservative).
            let approx_tokens = prompt_tokens as u64;
            // TURN ADMISSION (event-driven, bounded by the request deadline) — the mind's act
            // gate, then permit-first, then lease+pin her page's slot and page its KV onto a
            // now-free slot. The returned guard holds all of it for the WHOLE generation
            // (bound into `_admission` at function scope above). A Turn keys on her MIND
            // alone (one page across every room); a Fork on its own (persona, room). One that
            // cannot name its ids passes `None` and stays unpinned. Non-page traffic takes
            // the permit only and lands on the scratch slot below.
            let parse = |id: Option<&str>| id.and_then(|id| uuid::Uuid::parse_str(id).ok());
            let turn_key = class.page_key(parse(request.persona_id.as_deref()), parse(request.room_id.as_deref()));
            let root = endpoints.root();
            // Discovery: install this server's slot pool on first use (probes /props).
            // Non-Turn traffic also needs the pool on a single-slot server: it
            // must save the activity resident before borrowing the only slot.
            let pool = self.ensure_slot_pool(&endpoints).await.map_err(|miss| {
                miss.probe(root);
                crate::ai::inference_error::InferenceError::Unavailable(format!(
                    "slot discovery refused generation at {root}: {} ({miss:?})",
                    miss.as_str(),
                ))
            })?;
            let adm = crate::inference::turn_admission::admit_turn(
                &self.concurrency,
                turn_key,
                Some(pool),
                &self.client,
                root,
                approx_tokens,
                crate::inference::turn_admission::AdmissionPatience::for_turn(),
            )
            .await
            .map_err(crate::ai::inference_error::InferenceError::Unavailable)?;
            let scratch = adm.scratch_slot();
            // Only a turn that names its page owns a slot. A Turn with no persona (or a Fork
            // with no room) used to go out UNPINNED with cache_prompt on whenever the pool had a
            // scratch slot, so the server placed it by similarity and could overwrite a
            // resident's KV (card f3ba3b56, item 4). It is placed like every other
            // unattributed request: the scratch slot, or on a pool without one, a slot its
            // admission borrowed (the resident saved and detached first) with the cache off.
            let keyed_turn = class.holds_a_page() && turn_key.is_some();
            if class.holds_a_page() && !keyed_turn {
                continuum_base::probe!(
                    class = "inference.turn.keyless",
                    persona = request.persona_id.as_deref(),
                    room = request.room_id.as_deref(),
                    purpose = request.purpose.as_deref().unwrap_or("none"), // unwrap_or: an unset purpose is itself the fact to see
                    "a page-holding turn named no page (a Turn without a persona, a Fork without a room): it runs on the scratch slot, or, with none, on a slot its admission borrowed after saving that resident, never persisting a cache there"
                );
            }
            let decided = crate::inference::slots::placement(keyed_turn, adm.slot(), scratch);
            if !decided.cache_prompt {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("cache_prompt".to_string(), json!(false));
                }
            }
            let placement = decided.slot;
            _admission = Some(adm);
            // The draft only while this turn decodes alone (`inference::speculation`).
            let others = self
                .permit_target
                .load(std::sync::atomic::Ordering::Relaxed)
                .saturating_sub(self.concurrency.available_permits() + 1);
            if let (Some(limit), Some(obj)) = (crate::inference::speculation::draft_limit(others), body.as_object_mut()) {
                obj.insert("speculative.n_max".to_string(), json!(limit));
            }
            // GLASS BOX (KV-reuse 0% hunt 2026-08-26): the whole cache-reuse win
            // rides on placement. Report class + room so a cached:0 streak names
            // WHICH activity thrashed — or which class strayed off scratch.
            continuum_base::probe!(
                class = "inference.slot_pin.decision",
                persona = request.persona_id.as_deref(),
                room = request.room_id.as_deref(),
                traffic = class.as_str(),
                // the caller, so an unmapped purpose that defaults to Background can be named and
                // mapped (BigMama, the 5090 2026-10-11: 41 background requests on a one-slot lane,
                // each saving the resident and forcing a re-prefill, with no way to say whose)
                purpose = request.purpose.as_deref().unwrap_or("none"), // unwrap_or: an unset purpose is itself the fact to see
                pinned = placement.is_some(),
                slot = placement.map(|s| s as u64),
                "id_slot decision — Turn pins its activity slot; every other class lands on scratch"
            );
            if let Some(slot) = placement {
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("id_slot".to_string(), json!(slot));
                }
                if let (true, Some(persona)) = (
                    keyed_turn,
                    request.persona_id.as_deref().and_then(|p| uuid::Uuid::parse_str(p).ok()),
                ) {
                    _in_flight = Some(crate::inference::llama_server::InFlightSlotGuard::register(persona, root, slot));
                }
            }
            if let Some(persona) = request.persona_id.as_deref() {
                // #139 overshoot alarm: name the RAG-budget bug BEFORE the
                // server rejects. With context shift disabled at spawn the
                // server 400s on overflow instead of silently amputating the
                // prompt's middle; this WARN turns that 400 from a mystery
                // into a diagnosis. Chars/4 is a deliberately conservative
                // token estimate — an alarm that only fires when the overshoot
                // is unambiguous.
                if let Some(served) = served_ctx_by_root().get(endpoints.root()) {
                    let approx_tokens = prompt_tokens;
                    if approx_tokens > *served as usize {
                        // #3847: the total was measured but never ATTRIBUTED, so
                        // "the prompt is 6x the window" could not be turned into
                        // "which message is it". Every component is individually
                        // bounded (fractions of the served window in
                        // `ContextBudget`), so an overshoot this large is many
                        // bounded parts summing with no total cap — and which
                        // parts is the whole question. Roles + the largest few,
                        // in approx tokens, cost nothing on the non-overshoot
                        // path because this block only runs when already over.
                        let (message_count, largest_messages) = body
                            .get("messages")
                            .and_then(|m| m.as_array())
                            .map(|msgs| {
                                let mut rows: Vec<(&str, usize)> = msgs
                                    .iter()
                                    .map(|m| {
                                        let role =
                                            m.get("role").and_then(|r| r.as_str()).unwrap_or("?"); // unwrap_or: a message without a role string is still worth sizing; "?" names it rather than dropping it
                                        let len = m
                                            .get("content")
                                            .and_then(|c| c.as_str())
                                            .map(|c| continuum_base::token_estimate::estimate_prompt_tokens(c) as usize)
                                            .unwrap_or(0); // unwrap_or: a message with no string content contributes 0 tokens to a total we are only explaining
                                        (role, len)
                                    })
                                    .collect();
                                rows.sort_by(|a, b| b.1.cmp(&a.1));
                                let rendered = rows
                                    .iter()
                                    .map(|(role, tokens)| format!("{role}={tokens}"))
                                    .collect::<Vec<_>>()
                                    .join(" ");
                                (msgs.len(), rendered)
                            })
                            .unwrap_or((0, String::new())); // unwrap_or: no messages array means nothing to attribute; the probe still reports the total
                        tracing::warn!(
                            probe_class = "serving.ctx_overshoot",
                            approx_tokens,
                            served_per_slot_ctx = *served,
                            message_count,
                            largest_messages = %largest_messages,
                            persona,
                            "prompt likely exceeds the served per-slot window — the RAG \
                             budget overshot what llama-server actually serves (#139); \
                             expect a context-size rejection, fix the budget not the server"
                        );
                    }
                }
            }
        }

        // LoRA page-in: project the persona's genome onto the serving backend as
        // the llama.cpp `lora` request-body extension — the SAME backend-extension
        // mechanism as `repeat_penalty` above. The integer `id` is the server-side
        // load-index (discovered via GET /lora-adapters), which the CUSTODIAN
        // assigns when it loads the adapter. `lora_scale_vector` emits an explicit
        // scale for EVERY loaded adapter (requested scale, else 0.0) so an EMPTY
        // genome serves true base: llama.cpp applies a loaded adapter at 1.0 for
        // any request that omits the field, so omission cannot mean "off" once one
        // is loaded — that omission was the no-op behind the LIFT=0 measurement
        // (base arm sent no field → silently ran WITH the gene). Capability is
        // discovered, not declared: it probes the endpoint and FAILS LOUD if the
        // backend can't page LoRA or a requested adapter isn't loaded. `None` only
        // when nothing is loaded (omit the field, no probe penalty for non-LoRA).
        let active = request.active_adapters.as_deref().unwrap_or(&[]);
        continuum_base::probe!(
            class = "inference.request.lora_resolve",
            adapters = active.len() as u64,
            "resolving genome adapters against the lane catalog"
        );
        if let Some(entries) = self.lora_scale_vector(active).await? {
            if let Some(obj) = body.as_object_mut() {
                obj.insert("lora".to_string(), json!(entries));
            }
        }
        continuum_base::probe!(
            class = "inference.request.lora_resolved",
            "genome adapters resolved — shaping the rest of the request"
        );

        // Finish the body for the wire (thinking policy, response_format, native tools,
        // tool-surface probe) — `inference::request_body`.
        // ONE snapshot decides both where this request goes and whether it may carry its tail;
        // a tail the destination cannot take is refused, never dropped (Codex on #5491).
        let route = self.route_now(model, &endpoints);
        crate::inference::request_body::finish_body(&self.config, &request, model, route.tail_tools, &mut body)?;

        // Make request — the endpoint base for THIS request's model. Normally the
        // runtime/config base; for the local serving gateway, a request for the
        // snapshot's VISION model (the #106 sidecar lane serving beside a
        // text-only mind) routes to the snapshot's verified `vision_base_url`.
        // Snapshot-driven: the daemon publishes the address only after `/props`
        // confirmed sight, so this can never aim pixels at a text lane.
        let url = route.base.chat_completions();

        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        // Only add Authorization header if provider requires auth
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        // Log the body size + model so post-mortem can reconstruct why a
        // stall happened (oversized prompt, wrong model, etc.). Kept at
        // info! because this is the one log line every failing-persona
        // investigation needs to see.
        let body_bytes = serde_json::to_vec(&body).map_err(|error| {
            crate::ai::inference_error::InferenceError::Protocol(format!(
                "could not encode generation request: {error}"
            ))
        })?;
        // Glass-box the FINAL provider payload, not the pre-projection request.
        // Hash encoded URLs without logging pixels, prompts, or remote URLs.
        let wire_image_hashes: Vec<String> = body.get("messages")
            .and_then(|v| v.as_array()).into_iter().flatten()
            .filter_map(|m| m.get("content").and_then(|v| v.as_array()))
            .flatten()
            .filter_map(|p| p.get("image_url").and_then(|v| v.get("url")).and_then(|v| v.as_str()))
            .map(|url| {
                use sha2::Digest;
                format!("{:x}", sha2::Sha256::digest(url.as_bytes()))
            }).collect();
        if !wire_image_hashes.is_empty() {
            continuum_base::probe!(
                class = "inference.prompt.media_wire",
                request_id = %request_id,
                model = model,
                image_count = wire_image_hashes.len() as u64,
                image_url_sha256 = ?wire_image_hashes,
                body_bytes = body_bytes.len() as u64,
                "final provider image payload; URL hashes correlate with retained source bytes without exposing content"
            );
        }
        clog_info!(
            "POST {} model={} body_bytes={} has_tools={} stream={}",
            url,
            model,
            body_bytes.len(),
            body.get("tools")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
                > 0,
            body.get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        );

        // Concurrency permit. Turn admission above already took it (permit-FIRST,
        // before leasing) on the llamacpp/slot path. Any other provider has no slots
        // to pin, so take the permit here — the gate still applies (a no-op for cloud's
        // large semaphore), held in `_admission` to the end of generation exactly like
        // the slot path. Acquire can't fail: the semaphore is never closed.
        if _admission.is_none() {
            let root = endpoints.root();
            _admission = Some(
                crate::inference::turn_admission::admit_turn(
                    &self.concurrency,
                    None,
                    None,
                    &self.client,
                    root,
                    0,
                    crate::inference::turn_admission::AdmissionPatience::for_turn(),
                )
                .await
                .map_err(crate::ai::inference_error::InferenceError::Unavailable)?,
            );
        }

        let exact_prompt_quote = self.config.single_resident_model && self.targets_local_serving_lane();
        // Pre-flight the single-resident gateway (the serving guard, now its own module:
        // `inference::serving_guard`). Refuses when the lane cannot guarantee `model` or
        // the prompt alone overflows the served slot (#175).
        crate::inference::serving_guard::guard_resident_model(
            &self.config,
            self.dedicated_lane,
            model,
            crate::inference::serving_guard::PromptCount::Estimated(if exact_prompt_quote { 0 } else { prompt_tokens }),
            request.persona_id.as_deref().unwrap_or("non-persona"), // unwrap_or: no persona = a non-persona caller in the refusal text
        )
        .await?;

        // The managed engine quotes the finalized body before generation; the heuristic
        // preflight above checks residency only so an overestimate cannot refuse a fit.
        // POST through the lane with the mid-relaunch retry (`inference::lane_send`).
        let response = crate::inference::lane_send::send_with_lane_retry(
            &self.config,
            request_builder,
            body_bytes,
            request.turn_bound,
            crate::inference::lane_send::FillReceipt {
                model,
                prompt_tokens,
                prompt_quote: exact_prompt_quote
                    .then_some(crate::inference::lane_send::PromptQuote {
                        client: &self.client,
                        dedicated_lane: self.dedicated_lane,
                        caller: request.persona_id.as_deref().unwrap_or("non-persona"), // no persona: label the non-persona caller in a refusal; not a budget default
                        reply: request.max_tokens.unwrap_or(0), // absent max_tokens reserves no explicit output allowance
                        request_id: &request_id,
                    }),
            },
        )
        .await?;

        // Consume the SSE stream — `inference::sse_stream` (watchdogs, prefill liveness,
        // token/tool accumulation). The locals below are what the inline loop bound.
        let local_lane = self.targets_local_serving_lane();
        let crate::inference::sse_stream::StreamOutcome {
            acc_content,
            acc_reasoning,
            acc_tools,
            finish_reason_str,
            stream_usage,
            stream_timings,
            resp_model,
            probe_persona,
        } = crate::inference::sse_stream::consume_sse_stream(
            &self.config,
            &request,
            local_lane,
            response,
            &sink,
        )
        .await?;
        let response_time_ms = start.elapsed().as_millis() as u64;

        // Separate reasoning from the answer AT THE BOUNDARY: a reasoning model's
        // `<think>…</think>` (or a server `reasoning_content`) is captured for the
        // #363: a stream that reached EOF with real output is proof of life for the
        // local lane — this is the streaming sibling of the blocking path's
        // `note_real_decode` (which streaming never stamped, leaving the citizens'
        // primary path invisible to the liveness record). It also ends any failure
        // streak. Gated on the local lane like the failure stamps above.
        let delivered_something =
            !acc_content.is_empty() || !acc_reasoning.is_empty() || !acc_tools.is_empty();

        // harness/memory and stripped from `text` so it can NEVER reach the room.
        let raw_content = acc_content;
        let (text, reasoning) = extract_reasoning(
            &raw_content,
            (!acc_reasoning.is_empty()).then_some(acc_reasoning.as_str()),
        );
        let mut finish_reason = finish_reason_str
            .as_deref()
            .map(|r| self.map_finish_reason(r))
            .unwrap_or(FinishReason::Unreported); // no marker arrived: say so, never claim a stop
        let generation_completed =
            finish_reason_str.is_some() && !matches!(finish_reason, FinishReason::Error);

        match crate::inference::llama_server::classify_real_decode(
            local_lane,
            delivered_something,
            finish_reason,
        ) {
            crate::inference::llama_server::RealDecodeOutcome::NotOurLane => {}
            // The output cap ended it on a HEALTHY lane — the allowance owns this, not
            // the backend. Surfaced so it is never silent, never charged to the lane.
            crate::inference::llama_server::RealDecodeOutcome::CappedBeforeOutput => {
                continuum_base::probe!(
                    class = "inference.lane.capped_before_output",
                    model = resp_model.as_deref().unwrap_or("<unknown>"), // Missing model stays explicitly unknown in telemetry; no control decision uses this label.
                    response_time_ms,
                    "the local lane hit its output cap having committed no answer — an \
                     allowance fault on a working backend, NOT lane wedge evidence"
                );
            }
            crate::inference::llama_server::RealDecodeOutcome::ProofOfLife => {
                crate::inference::llama_server::note_real_decode();
            }
            crate::inference::llama_server::RealDecodeOutcome::DeliveredNothing => {
                // A stream that reached EOF CLEANLY having delivered nothing is a FAILED
                // real generation, not a quiet one — and it was the only wedge class the
                // liveness record could not see. The failure stamps in `sse_stream` fire on
                // transport death (read error, mid-stream silence, non-200); a latched Metal
                // backend does none of that. It answers 200, streams a well-formed SSE body
                // with zero deltas, and closes. Success was correctly withheld here (the
                // guard above), but nothing recorded a FAILURE either — so `REAL_DECODE_FAILS`
                // stayed at 0 and the heartbeat's wedge evidence, which outranks both the
                // recent-decode trust window and a passing 1-token smoke probe, never fired.
                //
                // Measured on the M5 2026-09-22, one llama-server log: 145
                // `kIOGPUCommandBufferCallbackErrorOutOfMemory` lines resolving to 2 root
                // Metal OOMs, each latching the backend ("backend is in error state from a
                // previous command buffer failure - recreate the backend to recover") which
                // the server never acts on. 113 tasks were then launched and released with
                // NO generation eval at all — prefill ran, generation never did. Downstream:
                // 73 `delib.empty_completion` faults across six citizens (Rania, Freya,
                // Mathis, Esme, Benchy, Cyrus), every one `reasoning_chars = 0`, each after
                // 30–190s of waiting. The deliberation faculty correctly refuses to launder
                // that as chosen silence, but the fault stayed PER-CITIZEN: six minds each
                // retried into the same dead backend for hours because no one told the LANE.
                //
                // Empty is the lane's failure, so the lane records it. At
                // `HEALTH_FAILS_TO_RELAUNCH` (2) consecutive, the heartbeat relaunches and
                // the Metal backend is recreated — the recovery the server's own log asks
                // for. Gated on `local_lane` like every other stamp, so a cloud provider's
                // empty turn can never smear the local record.
                // [[the-measurement-already-exists-and-nothing-consumes-it]]
                crate::inference::llama_server::note_real_decode_failure();
                continuum_base::probe!(
                    class = "inference.lane.delivered_nothing",
                    model = resp_model.as_deref().unwrap_or("<unknown>"), // Missing model stays explicitly unknown in telemetry; no control decision uses this label.
                    response_time_ms,
                    consecutive =
                        crate::inference::llama_server::consecutive_real_decode_failures(),
                    "the local lane streamed a clean EOF with no content, no reasoning and \
                     no tool call — counted against the lane, not the citizen"
                );
            }
        }

        // Assemble native tool calls from the streamed fragments.
        let mut tool_calls: Option<Vec<ToolCall>> = if acc_tools.is_empty() {
            None
        } else {
            let calls: Vec<ToolCall> = acc_tools
                .into_iter()
                .filter(|t| !t.name.is_empty())
                .map(|t| {
                    // A model's tool arguments that do not parse as JSON are a REAL
                    // failure of the generation, and this seam is the only place that
                    // knows it. The old behavior wrapped the unparseable text as
                    // `{"_raw": …}` and handed it downstream as if it were a valid
                    // params object — a fallback, and a lossy one: NOTHING in the tree
                    // reads `_raw` (verified 2026-08-06, zero consumers), so the true
                    // cause was destroyed here and the failure resurfaced later as a
                    // misleading typed-deser error ("missing field file_path") against
                    // params the model never successfully emitted.
                    //
                    // Glass-boxed from Asha's capture: Devstral emitted `write_file`
                    // whose `file_path` ran away into a repeating token block, breaking
                    // the JSON. The turn showed a confusing downstream error instead of
                    // "your tool arguments were not valid JSON."
                    // [[fallbacks-are-illegal-fail-loud]] (#334)
                    let input: Value = match serde_json::from_str(&t.arguments) {
                        Ok(v) => v,
                        Err(e) => {
                            continuum_base::probe!(
                                class = "ai.tool_call.unparseable_args",
                                tool = t.name.as_str(),
                                error = e.to_string().as_str(),
                                arg_len = t.arguments.len(),
                                // The whole blob rides in the call's result below
                                // (`__malformed_tool_arguments.raw`); the probe names its size.
                                "model emitted tool arguments that are not valid JSON — the call cannot be honored as written",
                            );
                            // Carry the parse error itself, under a SELF-DESCRIBING key,
                            // so whatever rejects this call downstream can say what
                            // actually went wrong instead of inventing a missing-field
                            // story about params that were never parsed.
                            json!({
                                "__malformed_tool_arguments": {
                                    "error": e.to_string(),
                                    "raw": t.arguments,
                                }
                            })
                        }
                    };
                    ToolCall {
                        id: t.id,
                        name: t.name,
                        input,
                    }
                })
                .collect();
            (!calls.is_empty()).then_some(calls)
        };

        // UNIVERSAL text-format tool-call fallback. When no NATIVE tool_calls came
        // back, scan the model's TEXT for `{"tool_call": {...}}` envelopes and lift
        // them into the canonical ToolUse shape — so the agent loop executes them
        // EXACTLY like native calls. Run REGARDLESS of declared protocol: the base
        // model picks the surface format (and a "native" gateway sometimes still
        // emits the call as content), so the adapter stays flexible and never lets a
        // persona's hands go dead over a formatting mismatch. Robust to malformed
        // siblings + multiple calls + ``` fences (see json_in_prompt_tools). A LoRA
        // can tighten the model to native later; this is the floor that always works.
        if tool_calls.as_ref().map_or(true, |t| t.is_empty()) {
            let parsed = super::super::json_in_prompt_tools::parse_tool_calls(&text);
            if !parsed.is_empty() {
                finish_reason = FinishReason::ToolUse;
                tool_calls = Some(parsed);
            }
        }

        // Build content blocks
        let mut content_blocks = Vec::new();
        if !text.is_empty() {
            content_blocks.push(ContentPart::Text { text: text.clone() });
        }
        if let Some(ref tcs) = tool_calls {
            for tc in tcs {
                content_blocks.push(ContentPart::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.input.clone(),
                });
            }
        }

        let usage = stream_usage
            .map(|u| UsageMetrics {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u
                    .total_tokens
                    .unwrap_or(u.prompt_tokens + u.completion_tokens),
                estimated_cost: None, // TODO: Calculate from model pricing
            })
            .unwrap_or_default();

        // Per-call PREFILL-vs-DECODE split for the speed harness. cache_n vs
        // prompt_n is the KV-cache hit/miss that dominates Metal wall-clock.
        if let Some(t) = &stream_timings {
            // The roster tile's speed needles — one map write per stream close.
            crate::inference::speed_pulse::record_speed(
                &probe_persona,
                t.predicted_per_second,
                t.prompt_per_second,
            );
        }
        let timing = stream_timings.map(|t| GenerationTiming {
            cached_tokens: t.cache_n,
            prefill_tokens: t.prompt_n,
            prefill_ms: t.prompt_ms,
            prefill_tokens_per_second: t.prompt_per_second,
            decode_tokens: t.predicted_n,
            decode_ms: t.predicted_ms,
            decode_tokens_per_second: t.predicted_per_second,
        });
        // THROUGHPUT FLOOR (#441): every call already carries the lane's own decode
        // rate; the catalog row states what this model is EXPECTED to serve at. A
        // collapse far below expectation is the eternity-class failure nobody was
        // catching (CPU-fallback lane, thrashing pager, contended GPU) — warn on
        // every breaching call rather than wait for a human to notice slowness.
        if let Some(t) = &timing {
            warn_if_decode_collapsed(model, t.decode_tokens, t.decode_tokens_per_second);
            // The measured prefill rate feeds the render budget (`inference::prefill_rate`):
            // what a turn may cost before its first token is derived from what this lane
            // actually prefills at, not from the served window alone.
            crate::inference::prefill_rate::observe(
                model,
                t.prefill_ms,
                t.prefill_tokens_per_second,
            );
        }

        // Plain EOF is accepted by the existing stream reader, but is not proof
        // that generation finished. Only an explicit successful terminal frame
        // makes the provisional physical KV attribution safe to retain.
        if generation_completed {
            if let Some(admission) = _admission.as_mut() {
                admission.record_served_prompt(
                    endpoints.root(),
                    resp_model.as_deref(),
                    usage.input_tokens,
                );
                admission.generation_completed();
                // THE RESTORE ECONOMY, JUDGED ON REAL TRAFFIC. A turn whose KV was
                // restored from a page either reused it (the engine reports cached
                // tokens) or silently re-prefilled everything (cache_n 0, the 09-04 /
                // 09-14 class). This is the evidence the deploy self-check reads
                // instead of injecting a probe into a live engine, which measured load
                // and contention and held the M5 three times on 2026-09-25/26 while
                // restores were warm (card 356ce732).
                // A restore the engine REFUSED is reported by turn admission at the decision
                // (outcome `restore_rejected`, the same class), so it counts even for a turn
                // that never completes; here only restores that happened are judged warm or
                // cold (card f3ba3b56). A resident prefix the engine did not reuse is its own
                // class: it is not a restore, and must not dilute the restore verdict.
                use crate::inference::turn_admission::PageIn;
                if let Some(t) = &timing {
                    match admission.page_in() {
                        PageIn::Restored => continuum_base::probe!(
                            class = "inference.restored_turn",
                            cached = t.cached_tokens,
                            prompt = t.prefill_tokens,
                            outcome = if t.cached_tokens > 0 { "warm" } else { "cold" },
                            "a turn restored from a KV page completed — did the engine reuse it"
                        ),
                        PageIn::Resident if t.cached_tokens == 0 => continuum_base::probe!(
                            class = "inference.resident_prefix_lost",
                            prompt = t.prefill_tokens,
                            "a turn whose KV was resident in its slot reused none of it: a full re-prefill"
                        ),
                        PageIn::Resident | PageIn::None | PageIn::RestoreRejected => {}
                    }
                }
            }
        }

        Ok(TextGenerationResponse {
            text,
            finish_reason,
            model: resp_model.unwrap_or_else(|| model.to_string()),
            provider: self.config.provider_id.to_string(),
            usage,
            response_time_ms,
            request_id,
            content: if content_blocks.is_empty() {
                None
            } else {
                Some(content_blocks)
            },
            tool_calls,
            reasoning,
            routing: None,
            error: None,
            timing,
        })
    }
}
