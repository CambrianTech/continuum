# Performance Harness Framework

> **Premise** (Joel, 2026-05-16): *"Ask for proof of performance concerns and then design harnesses."*
>
> **Status.** Design proposal. Harnesses are designed against the substrate's named performance covenants and Joel's directive that VDD-record output replaces handwritten timing reports.
>
> **Companion to** [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) §"Standard VDD Record" + §"One-Line Instrumentation API" and [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) Performance Budget tables per Part.

## Why This Document Exists

The architecture docs name performance covenants: RAG composition < 500ms, vector search < 50ms, voice response < 3s, persona tick < 1ms, recall hot-path < 5ms on Air, working-set page-in < 1ms, governor `current_policy()` < 50ns, and many more per-part budgets in `GENOME-FOUNDRY-SENTINEL.md`. **They are claims until they are measured.** This document specifies the harnesses that turn the claims into evidence.

Three principles:

1. **Harnesses produce VDD records, not prose reports.** The substrate's Standard VDD Record format (`CBAR-SUBSTRATE-ARCHITECTURE.md` §"Standard VDD Record") is the output of every harness. Humans paste it into PR comments; machines consume the JSONL form for regression detection. No harness invents its own output schema.
2. **Per-anchor scoping.** Every harness runs against the substrate's two hardware anchors (MacBook Air UMA-16, RTX 5090 discrete-32+64) at minimum. Intermediate hardware classes interpolate; explicit hardware-class entries can be added per harness as evidence accumulates.
3. **Baseline-relative, not absolute.** A harness's pass/fail is *relative to a committed baseline*, not to a hand-written budget. Budgets bound expectations; baselines are the regression line. Two PRs ago is the right comparison, not last year's wishful thinking.

## The Standard VDD Record (Recap)

Every harness emits records of this shape. The schema lives in `CBAR-SUBSTRATE-ARCHITECTURE.md`; reproducing inline so this doc is self-contained:

```text
scenario:               # harness-specific scenario name
platform:               # macos / linux / windows / vision-pro / ...
hardware:               # silicon-model + vram + ram + power source + thermal class
backend:                # metal / cuda / vulkan / cpu
git_sha:                # commit under test
command:                # what was run
model:                  # which model variant
gpu_layers:
unsupported_layers:
cold_start_ms:
first_token_ms:
first_response_ms:
all_responses_ms:
responses_expected:
responses_observed:
silence_reasons:        # typed reasons for any silent outputs
tok_per_sec:
cpu_pct_avg:
cpu_pct_peak:
rss_mb:
gpu_util_pct_avg:
gpu_memory_mb:
queue_wait_ms:
execution_ms:
coalesced_count:
deferred_count:
stale_drop_count:
error_count:
degraded_reason:        # typed if any degradation triggered
log_refs:               # references to deep logs for debugging
next_bottleneck:        # the harness's own observation of what to investigate next
policy_version:         # governor policy at test time (from #1335 hardware probe + #1345 governor)
cascade_step:           # cascade step at test time
```

Every field has a value or an explicit `null`-with-reason. No silent gaps.

## Harness Anatomy

A harness is a Rust binary or `cargo test` target with four well-defined parts:

```rust
// PROPOSED — src/workers/continuum-core/tests/harness/<harness-name>.rs

// PART 1 — Setup. Bring the substrate up in a known state.
//                 Use the test-substrate fixtures (no live network unless declared).
fn setup() -> SubstrateUnderTest {
    let cfg = HarnessConfig::from_env();                            // CONTINUUM_HARNESS_HARDWARE_CLASS, etc.
    let substrate = SubstrateUnderTest::boot(cfg)
        .with_hardware_anchor(HardwareAnchor::detect())             // Air or 5090 detected at runtime
        .with_governor_policy(GovernorPolicy::for_anchor(&anchor))  // honest policy for this hardware
        .with_isolated_data_dir()                                    // never touch the user's longterm.db
        .ready();
    substrate
}

// PART 2 — Scenario. The actual operation being measured.
//                    Wrapped in vdd_scope! so the substrate captures timing automatically.
async fn scenario(substrate: &SubstrateUnderTest) -> Result<ScenarioResult, HarnessError> {
    let _span = vdd_scope!(substrate.ctx, "<harness-name>", ResourceClass::<Lane>);
    // do the work; the scenario emits typed records via the trace bus
    // as the substrate does its job
}

// PART 3 — Measurement. Pull the VDD record from the trace bus.
fn measure(substrate: &SubstrateUnderTest) -> VddRecord {
    substrate.collect_vdd_records()
        .filter(|r| r.scenario == "<harness-name>")
        .into_record()                                              // produces the Standard VDD Record
}

// PART 4 — Compare. Against the committed baseline; emit pass/fail with delta.
fn compare(record: &VddRecord, baseline: &VddRecord) -> HarnessOutcome {
    HarnessOutcome::new(record, baseline)
        .with_regression_tolerance(0.10)                            // 10% slower = warn; 25% slower = fail
        .with_explicit_failure_budgets()                            // some fields are hard ceilings, not relative
        .resolve()
}
```

Each harness ships:

- One `.rs` file (≤ 200 lines including helpers)
- A baseline JSON record per hardware anchor (`tests/harness/baselines/<harness>.air.json`, `<harness>.rtx5090.json`)
- An entry in `Cargo.toml` declaring the harness as a `[[bin]]` or `[[test]]`
- An entry in `tests/harness/manifest.toml` declaring its cadence (per-PR / weekly / nightly)
- An entry in this document under §"Harness Catalog"

## Per-Anchor Scoping

The substrate's two anchor configurations are the harness's two default scopes. Every harness runs against both unless the scenario only makes sense on one (e.g. a UMA-specific paging test).

| | **Air (UMA, 16 GB)** | **RTX 5090 (discrete, 32+64 GB)** |
|---|---|---|
| Identifier | `air-m-uma-16` | `rtx-5090-32-64` |
| Baseline location | `tests/harness/baselines/<harness>.air-m-uma-16.json` | `tests/harness/baselines/<harness>.rtx-5090-32-64.json` |
| Default cadence | weekly | per-PR (when Rust files touched) |
| CI runner | dedicated Mac M-series (if available) or marked `[ignored]` | dedicated Linux+5090 runner or marked `[ignored]` |

A harness whose Air baseline is missing skips on Air with explicit `[Skipped: NoAirBaseline]` — never silently passes. Adding the baseline is a separate PR; first run produces a "candidate baseline" the human reviews + commits.

Intermediate hardware (M-Pro/Max, AMD ROCm, Vulkan-only Intel) gets baselines added per-harness as evidence accumulates. The framework supports `N` baselines per harness, not just 2.

## Harness Catalog

The harnesses below are designed against the substrate's named performance covenants. The list is a starting set; specific concerns from the airc room (see §"Pending Evidence-Driven Additions") will add more.

### `cold-start-harness`

Measures time from process exec to first usable substrate. Hard ceiling per CBAR-SUBSTRATE: < 30s before missing-artifact health surface fires.

| Aspect | Value |
|---|---|
| Scenario | `cargo run --bin continuum-core --release` with a clean test data dir + Qwen3-7B-Q4K artifact present |
| Key fields | `cold_start_ms`, `first_token_ms`, `rss_mb` at ready, `gpu_memory_mb` at ready |
| Pass threshold (Air) | `cold_start_ms < 30000` (hard ceiling); `first_token_ms < 8000` (substrate-claim) |
| Pass threshold (5090) | `cold_start_ms < 10000`; `first_token_ms < 3000` |
| Cadence | per-PR for Rust changes; nightly absolute |
| Baseline location | `tests/harness/baselines/cold-start.*.json` |

### `persona-tick-harness`

Measures the substrate's claim that persona scheduling ticks are < 1ms. Verifies CBAR-SUBSTRATE's RTOS rule that the hot path can't block on background work.

| Aspect | Value |
|---|---|
| Scenario | Boot substrate with 4 personas + 2 background modules; record per-tick wall-clock for 1000 ticks under no-load, then under simulated chat pressure |
| Key fields | `tick_p50_us`, `tick_p99_us`, `tick_max_us` (new VDD record fields proposed for this harness; see §"Schema Extensions") |
| Pass threshold (Air) | `tick_p99_us < 1500` (50% slack on the < 1ms claim) |
| Pass threshold (5090) | `tick_p99_us < 800` |
| Cadence | per-PR for runtime changes; weekly otherwise |
| Baseline location | `tests/harness/baselines/persona-tick.*.json` |

### `rag-composition-harness`

Measures CBAR-SUBSTRATE's < 500ms RAG composition claim. Drives the rag-composer module from §"Module Catalog II".

| Aspect | Value |
|---|---|
| Scenario | Persona issues a `WorkingMemoryAssemblyRequest` against 12 conversation history sources + 4 hippocampus engrams; composer composes; measure end-to-end |
| Key fields | `composition_ms`, `sources_loaded`, `engrams_pulled`, `queue_wait_ms`, `cache_hit` (boolean), `policy_version`, `cascade_step` |
| Pass threshold (Air) | `composition_ms < 500` cold; `< 100` cache hit |
| Pass threshold (5090) | `composition_ms < 200` cold; `< 50` cache hit |
| Cadence | per-PR for cognition/genome changes; weekly otherwise |
| Baseline location | `tests/harness/baselines/rag-composition.*.json` |

### `vector-search-harness`

Measures CBAR-SUBSTRATE's < 50ms vector search claim. Drives `demand-aligned-recall` against a synthetic engram store of 10k engrams.

| Aspect | Value |
|---|---|
| Scenario | Synthetic store of 10k engrams (1024-dim embeddings); 100 randomized queries; measure each end-to-end |
| Key fields | `search_p50_ms`, `search_p99_ms`, `cache_hit_rate`, `ann_index_warm` (boolean) |
| Pass threshold (Air) | `search_p99_ms < 50` (governor policy honored) |
| Pass threshold (5090) | `search_p99_ms < 10` |
| Cadence | per-PR for genome/recall changes; weekly otherwise |
| Baseline location | `tests/harness/baselines/vector-search.*.json` |

### `voice-response-harness`

Measures CBAR-SUBSTRATE's < 3s voice response claim. Drives the full chain: audio in → VAD → STT → cognition → composer → TTS → audio out.

| Aspect | Value |
|---|---|
| Scenario | Pre-recorded 5-second audio clip; substrate runs the chain end-to-end; measure first-byte-of-audio-out |
| Key fields | `vad_ms`, `stt_ms`, `cognition_ms`, `composition_ms`, `tts_first_audio_ms`, `total_voice_response_ms` |
| Pass threshold (Air) | `total_voice_response_ms < 3500` (slight slack; the < 3s claim is the 5090 target) |
| Pass threshold (5090) | `total_voice_response_ms < 2000` |
| Cadence | weekly (full chain is slow + flaky to run per-PR) |
| Baseline location | `tests/harness/baselines/voice-response.*.json` |

### `consolidation-phase-harness`

Measures the sleep / consolidation cycle's resource shape per `GENOME-FOUNDRY-SENTINEL.md` §"Sleep / consolidation". Critical for the persona-thought-process's deep-thought-during-sleep claim.

| Aspect | Value |
|---|---|
| Scenario | Substrate with 1000 buffered traces; trigger `ConsolidationPhase`; measure sentinel refinement + engram clustering + LoRA fine-tune attempts; assert governor doesn't get into a cascade > 2 during consolidation |
| Key fields | `consolidation_total_ms`, `engrams_clustered`, `lora_finetune_count`, `lora_finetune_validation_pass_count`, `lora_finetune_validation_fail_count`, `max_cascade_step_during_phase` |
| Pass threshold (Air) | `consolidation_total_ms < 1.8e6` (30 min budget); `max_cascade_step_during_phase ≤ 2` |
| Pass threshold (5090) | `consolidation_total_ms < 6e5` (10 min); `max_cascade_step_during_phase ≤ 1` |
| Cadence | nightly (slow harness; only meaningful at full scale) |
| Baseline location | `tests/harness/baselines/consolidation-phase.*.json` |

### `multi-persona-contention-harness`

Measures behavior when N personas in one room all touch the same frame. Validates the persona-cognition-contract's "real inbox, real working memory, real budget" invariants A1–A3 under load, and the prefix-share KV cache win (Part 8) for group conversations.

| Aspect | Value |
|---|---|
| Scenario | N=8 personas in one room; one frame arrives; measure per-persona completion + total VRAM peak + prefix-cache hit rate |
| Key fields | `per_persona_total_ms[]`, `peak_vram_mb_total`, `kv_prefix_share_hit_rate`, `inbox_isolation_violations` (must be 0) |
| Pass threshold (Air) | `peak_vram_mb_total < 14000` (substrate honors UMA budget); `inbox_isolation_violations == 0` |
| Pass threshold (5090) | `peak_vram_mb_total < 30000`; `kv_prefix_share_hit_rate > 0.6` |
| Cadence | weekly |
| Baseline location | `tests/harness/baselines/multi-persona-contention.*.json` |

### `federation-gossip-harness`

Measures GENOME-FOUNDRY-SENTINEL §"Performance Budget" gossip claims. Two synthetic peer instances; gossip-summary exchange round.

| Aspect | Value |
|---|---|
| Scenario | Boot 2 substrate instances on same host (different ports); each populates 500 artifact summaries; run one gossip round; measure exchange + diff resolution |
| Key fields | `gossip_round_ms`, `summary_diff_count`, `conflict_resolution_count`, `bytes_exchanged` |
| Pass threshold (Air) | `gossip_round_ms < 5000` |
| Pass threshold (5090) | `gossip_round_ms < 5000` (same target — bounded by network not compute) |
| Cadence | weekly |
| Baseline location | `tests/harness/baselines/federation-gossip.*.json` |

### `speculation-hit-rate-harness`

Measures Part 9 speculation. Validates that hit-rate-feedback to the governor produces the documented oscillation-free behavior.

| Aspect | Value |
|---|---|
| Scenario | Persona runs through a scripted 50-turn conversation with predictable next-turn patterns; substrate's speculator generates branches; measure hit-rate over the run + governor cascade-step transitions |
| Key fields | `hit_rate`, `branches_generated`, `branches_hit`, `branches_discarded`, `bytes_wasted_on_misses`, `cascade_step_oscillations` (must be 0) |
| Pass threshold (Air) | `hit_rate > 0.4`; `cascade_step_oscillations == 0` |
| Pass threshold (5090) | `hit_rate > 0.6`; `cascade_step_oscillations == 0` |
| Cadence | weekly |
| Baseline location | `tests/harness/baselines/speculation-hit-rate.*.json` |

### `reprojection-confidence-harness`

Validates CBAR-SUBSTRATE §"Spatiotemporal Reprojection". A slow inference at T returns at T+1.5s; reprojection picks the correct transform + confidence given recorded deltas.

| Aspect | Value |
|---|---|
| Scenario | Inject a synthetic 1.5s-delayed result with known T-state + T+Δ-state; substrate reprojects via toolkit; assert correct transform variant + confidence in expected range |
| Key fields | `reprojection_transform_variant`, `reprojection_confidence`, `stale_returned_count` (must be 0 unless delta exceeds reprojection tolerance) |
| Pass threshold (both anchors) | Correct variant per scenario class; confidence within `±0.05` of expected; no silent stale returns |
| Cadence | per-PR for reprojection changes; weekly otherwise |
| Baseline location | `tests/harness/baselines/reprojection-confidence.*.json` |

### `governor-cascade-harness`

Validates Part 11 governor cascade with hysteresis + restore-speculation-last anti-oscillation rule.

| Aspect | Value |
|---|---|
| Scenario | Boot substrate at cascade 0; inject simulated pressure signals (thermal escalation, then clearing); record cascade-step transitions + speculation level over the run |
| Key fields | `cascade_step_transitions`, `time_at_each_step_ms`, `speculation_restored_step_delay`, `oscillation_count` (must be 0) |
| Pass threshold (both anchors) | Transitions match documented thresholds + hysteresis gaps; `speculation_restored_step_delay >= 1`; `oscillation_count == 0` |
| Cadence | per-PR for governor changes; weekly otherwise |
| Baseline location | `tests/harness/baselines/governor-cascade.*.json` |

### `audit-recorder-roundtrip-harness`

Smoke harness validating the substrate's no-silent-fallback invariants at the audit layer. Now that `#1344 audit-recorder` shipped, this harness gates regressions.

| Aspect | Value |
|---|---|
| Scenario | Substrate runs 1000 turns with mixed outcomes (200 refusals, 100 governor-overrides, 50 federation-policy-drifts, 800 access-denied attempts, 50 threat-detections); assert all land in `audit_archive.jsonl` with valid signatures |
| Key fields | `audit_entries_recorded`, `audit_signature_failures` (must be 0), `audit_mutation_attempts_rejected` (proves append-only) |
| Pass threshold (both anchors) | All 1200 expected entries present; zero signature failures; all mutation attempts rejected with typed `AppendOnly` error |
| Cadence | per-PR (this is cheap + load-bearing) |
| Baseline location | `tests/harness/baselines/audit-recorder.*.json` |

## Schema Extensions

The Standard VDD Record covers most needs but some harnesses add typed fields. New fields go in:

```rust
// PROPOSED — src/workers/continuum-core/src/vdd/schema_extensions.rs
pub struct VddRecordExtensions {
    pub tick_metrics:           Option<TickMetrics>,                 // persona-tick-harness
    pub composition_metrics:    Option<CompositionMetrics>,          // rag-composition-harness
    pub recall_metrics:         Option<RecallMetrics>,               // vector-search-harness
    pub voice_chain_metrics:    Option<VoiceChainMetrics>,           // voice-response-harness
    pub consolidation_metrics:  Option<ConsolidationMetrics>,        // consolidation-phase-harness
    pub contention_metrics:     Option<ContentionMetrics>,           // multi-persona-contention-harness
    pub federation_metrics:     Option<FederationMetrics>,           // federation-gossip-harness
    pub speculation_metrics:    Option<SpeculationMetrics>,          // speculation-hit-rate-harness
    pub reprojection_metrics:   Option<ReprojectionMetrics>,         // reprojection-confidence-harness
    pub cascade_metrics:        Option<CascadeMetrics>,              // governor-cascade-harness
    pub audit_metrics:          Option<AuditMetrics>,                // audit-recorder-roundtrip-harness
}
```

Each extension struct is small (typically 5–10 fields). The base VDD Record stays uniform; extensions land alongside the harness that needs them.

## Regression Detection

Two layers of pass/fail per harness:

### Layer 1: Hard Ceilings

Some fields have hard ceilings derived from substrate covenants (e.g. `tick_p99_us < 1500` on Air). A harness that fails a hard ceiling **fails the PR regardless of baseline**. The covenant is the law; baselines drift around it but never cross it.

### Layer 2: Baseline Delta

For non-ceiling fields (e.g. `composition_ms`, `gpu_memory_mb`), the harness compares to the committed baseline:

| Delta | Action |
|---|---|
| `≤ 5% slower` | Pass; no action |
| `5–10% slower` | Pass with warning in PR comment |
| `10–25% slower` | Pass with warning + flag for review |
| `> 25% slower` | Fail the harness; PR cannot merge without override |
| `≥ 5% faster` | Pass + automatic baseline-update suggestion in PR comment |

Baselines are committed JSON files. Updating a baseline is a separate, reviewable action — never silent. A PR that wants to "claim" a baseline update must do so explicitly with `tests/harness/baselines/<harness>.<anchor>.json` in the diff and a justification comment.

## CI Integration

Harnesses are tagged by cadence:

| Cadence | When it runs | Examples |
|---|---|---|
| `per-pr` | Every PR touching relevant files (Rust source for cognition/genome/runtime/governor) | `cold-start`, `persona-tick`, `audit-recorder-roundtrip`, `governor-cascade` (when governor changes) |
| `weekly` | Scheduled GitHub Action; merged-to-canary trigger | `rag-composition`, `vector-search`, `multi-persona-contention`, `federation-gossip`, `speculation-hit-rate`, `voice-response` |
| `nightly` | Scheduled, full-substrate runs | `consolidation-phase`, full-chain integration scenarios |
| `release` | Pre-tag gate | All harnesses; baselines refreshed; release notes include VDD record summary |

A `cargo continuum-vdd <harness>` invocation runs any harness locally. CI uses the same binary — same Rust code, no test-harness duplication.

## Harness Output Bundle

A harness run produces three artifacts:

1. **The VDD Record (JSONL)** — pasted into the PR comment by the CI action; consumed by regression detection.
2. **The Reproducibility Manifest (TOML)** — `git_sha`, `policy_version`, `cascade_step`, environment variables that affected the run, hardware-class detection result, seed values for any randomness. Sufficient to replay the harness deterministically.
3. **The Human-Readable Summary (Markdown)** — table of pass/fail per field with the delta vs baseline highlighted. Reviewer-friendly.

All three live under `~/.continuum/vdd/<sha>/<harness>/`. CI uploads them as artifacts on every run. Old runs evict after 90 days; baselines never evict.

## Pending Evidence-Driven Additions

The harness catalog above is the design floor. Specific concerns from the airc room — once they land in response to the perf evidence request — will add to it. This section is a placeholder:

> **(filled in as evidence arrives — claude-tab-1, codex, vhsm-d1f4, others)**
>
> Pending: slowest wall-clock paths observed in canary, regressions noticed in the last week of merges, resource pressure incidents, what can't currently be measured, what's budgeted but unverified, hardware-class gaps.
>
> Each concrete data point becomes either (a) a new harness in the catalog, or (b) a sharpened pass-threshold on an existing one, or (c) a new field in the VDD schema extensions.

## Acceptance Criteria For The Framework Itself

The harness framework is "done" when:

- A `cargo continuum-vdd <harness>` binary exists; running it produces all three output artifacts.
- The framework's own infrastructure (baseline loader, regression detector, JSONL writer, anchor detector) lives in `src/workers/continuum-core/src/vdd/` and is itself test-covered.
- Two anchor baselines (`air-m-uma-16`, `rtx-5090-32-64`) exist for at least the `per-pr`-cadence harnesses.
- CI runs `per-pr` harnesses on every Rust-touching PR and posts the result as a PR comment with VDD record + delta highlights.
- A regression that fails a hard ceiling blocks merge; a regression that exceeds 25% on a baseline-relative field blocks merge.
- The framework's own performance budget is honored: harness overhead (setup + measurement + compare, excluding the scenario itself) < 50 ms per run.

## Open Questions

1. **Where do the harnesses live in the workspace?** `tests/harness/` per-crate, or a top-level `harnesses/` crate? Tentative: top-level `harnesses/` crate that depends on continuum-core; that lets harnesses share the framework infrastructure without polluting any one crate's test surface.

2. **Hardware availability for CI.** The Air + 5090 anchors are aspirational unless we have CI runners with that hardware. Tentative: any harness without a runner is marked `[ignored]` and produces "candidate baselines" when manually run; humans commit the baselines until CI infrastructure catches up.

3. **How to handle noisy harnesses.** Some scenarios (multi-persona-contention, federation-gossip) are inherently variable. Tentative: harness records P50 + P99 + P99.9 instead of a single mean; regression detection uses P99 by default but harness can opt into P50-relative for stability-shaped metrics.

4. **Baseline update authority.** Who is allowed to update a baseline? Tentative: any peer with merge rights; updates are reviewable like any PR; a baseline update must include a justification (PR description explains what changed and why the new number is the new normal).

5. **Cross-harness regression detection.** Sometimes a regression appears in one harness because of a change visible in another. Tentative: the regression report includes "related-harness deltas" — if cold-start got 15% slower AND rag-composition got 10% slower in the same PR, both deltas appear in the PR comment so the reviewer sees the correlation.

6. **Per-persona-shape harnesses.** Different personas have different working-set sizes / model preferences / cadences. Should there be per-persona-shape harnesses? Tentative: yes, but not in v1. v1 uses a generic "code-reviewer" persona shape. v2 adds shapes for chat-reactive, vision-aware, voice-realtime, etc.

## See Also

- [CBAR-SUBSTRATE-ARCHITECTURE.md](CBAR-SUBSTRATE-ARCHITECTURE.md) §"Standard VDD Record" + §"One-Line Instrumentation API"
- [GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md) Performance Budget tables per Part
- [PERSONA-COGNITION-CONTRACT.md](PERSONA-COGNITION-CONTRACT.md) §"Acceptance Criteria" — the harnesses verify these claims
- [MODULE-CATALOG.md](MODULE-CATALOG.md) §"Next Modules To Build" — the modules these harnesses validate
- [ALPHA-GAP-ANALYSIS.md](../planning/ALPHA-GAP-ANALYSIS.md) — Lane C VDD telemetry substrate is the foundation this framework lives on
