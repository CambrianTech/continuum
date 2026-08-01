# Expert-Pager Policy Prototypes (reference for the TierPolicy port)

Std-only Rust reference implementations of the MoE residency policy, written on
BigMama against live K3 traces. **These are the source of truth for M5's port of
the learned policy behind `TierPolicy`** (`core/continuum-core/src/capacity/expert_tier_policy.rs`).
Port faithfully first — the measured numbers below are properties of these exact
constants + reward math — then improve.

| File | What it is | Measured |
|---|---|---|
| `trace_replay.rs` | Replays an ordered expert-access trace (`GGML_MOE_TRACE_FILE`), scores residency policies | recency window beats LFU 3-4x; reuse compounds with gen length |
| `predictor.rs` | Offline learned-decay predictor (recency↔frequency EMA) | +5 pts held-out over pure recency |
| `online_predictor.rs` | Online bandit over decay candidates (the v2 `TierPolicy` body) | beats best-fixed on non-stationary: 49.8% vs 47.8% |
| `self_optimize.rs` | Joint speed×quality objective sweep (cruft-fraction ↔ tok/s ↔ quality) | ~1.2 tok/s @ 88% quality balanced (modelled) |

Reward signal = the live per-token `PagerCaptureEvent` (fetched_bytes / fault_ms / tok_s).
Actuator knobs the policy emits (v1 control-file `GGML_MOE_PLAN_FILE`): `budget_bytes`,
`window_k`, `pin_list[{layer,expert}]`. See `docs/architecture/EXPERT-PAGING-CONTROL-LAW.md`.
