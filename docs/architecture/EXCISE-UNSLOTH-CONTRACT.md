# Excise unsloth — the two contracts, the coupling map, the honest sequence

**Status:** in progress (#53). This doc is the single source of truth for the
excision. It exists because the work sprawled across 15 files and a hidden
dependency (#55) before it was mapped. Read this before touching any
`unsloth_control` / `"unsloth"`-id site.

> "unsloth" is not a type. It is (1) a provider-registry **string** behind the
> generic `OpenAICompatibleAdapter`, and (2) a control module
> (`inference/unsloth_control.rs`) that loads/probes a local OpenAI-compatible
> server. Excising it = routing every consumer onto two clean contracts, then
> deleting the module. No behavior is lost; it is **relocated to a contract**.

## The two contracts everything routes onto

Every scattered coupling below resolves to exactly one of these. If a change
doesn't route onto A or B, it is not part of the excision — it is scope creep.

### Contract A — the Serving Seam  `inference::llama_server`  ✅ BUILT
The single answer to *"what model is served, and where?"*

```
ServingSnapshot { active_model: Option<String>, ready: bool, base_url: String }
current_serving() -> ServingSnapshot          // sync, no-wait, hot path
await_ready_serving(timeout) -> Option<…>      // async, waits for first reconcile
DEFAULT_SERVING_WAIT: Duration                 // the ONE shared readiness bound
```

Published by `ServingDaemonModule` on a `watch` + `serving.snapshot` bus event.
**Rule: subscribers READ the snapshot; they do NOT each issue their own HTTP
probe.** This replaces `unsloth_base_url()`, `UnslothHttp::list_models()`,
`DEFAULT_HOST`, `ensure_startup_model()`, and `ensure_api_key()`.

### Contract B — the Adapter Capability Surface  (#55)  ✅ BUILT  `aa3e4c26d`
The single answer to *"what can this adapter/model do?"* The provider-id string
`"unsloth"` was a **stand-in** for four capabilities, branched on inline. They are
now DECLARED in the registry (`Provider.capabilities: ProviderCapabilities`) and
CONSUMED by the adapter — never an `id == "..."` compare:

```
ProviderCapabilities {              // model_registry/types.rs — Default = cloud case
  tool_protocol: ProviderToolProtocol,   // Native | JsonInPrompt
  suppress_thinking: bool,
  supports_embeddings: bool,
  supports_image_generation: bool,
  single_resident_model: bool,      // pre-flight ensure_model_active before generate
}
```

| Former site (`ai/openai_adapter.rs`) | Now reads |
|---|---|
| `tool_protocol` id-branch | `provider.capabilities.tool_protocol` |
| `ThinkingMode` id-branch | `provider.capabilities.suppress_thinking` |
| `supports_embeddings` / `supports_image_generation` | `self.config.supports_*` |
| per-generation `ensure_model_active` gate | `self.config.single_resident_model` |

The surface GROWS by adding a `#[serde(default)]` field, never an id branch.
Declared on two outliers only (openai, unsloth); the other 10 providers inherit
defaults. **The provider id is no longer load-bearing for behavior — slice 3b's
rename is now a pure string change.** Carry `lora_hotswap` here when failover
lands ([[seamless-persona-failover-model-and-genome]]).

## Coupling map — every site, its owner, its contract, its gate

| File | Coupling | Concern | Routes onto | State |
|---|---|---|---|---|
| `inference/llama_server.rs` | the seam + `DEFAULT_SERVING_WAIT` | **Contract A itself** | — | ✅ `be1a90003` |
| `persona/supervisor.rs` | `ServedModelPersonaAdapterFactory` | persona upstart bind | A | ✅ `df42bf18e` |
| `cognition/inference_session.rs` | `resolve_model` (✅) · label → `PROVIDER_ID` | lease resolve · label | A · const | ✅ `ea070942a` |
| `modules/ai_provider.rs` | boot announce (✅) · register gated on A · base_url from snapshot | gateway announce · register | A | ✅ `ea070942a` |
| `ai/openai_adapter.rs` | id-keyed behavior → `provider.capabilities` (B) · single-resident pre-flight → `current_serving()` (A) | **adapter behavior** | **B (#55)** + A | ✅ `aa3e4c26d` + `cd440d4d4` |
| `model_registry/catalog.rs` | `capabilities` declared · `id:PROVIDER_ID` · `auth:None` | catalog entry | A (base) + B (caps) | ✅ `ea070942a` |
| `cognition/generate_response.rs` | `DEFAULT_GENERATE_PROVIDER = PROVIDER_ID` | default route id | const | ✅ `ea070942a` |
| `main.rs` | `ensure_startup_model()` spawn DELETED | startup model load | A (daemon owns) | ✅ `715da98cf` |
| `ipc/mod.rs` | `ensure_api_key`/`ApiKeyStatus` boot check DELETED | boot key check | none (no key) | ✅ `715da98cf` |
| `inference/model_commands.rs` | `unsloth_control` keystone `:42` | model load/unload cmds | forge/serving | #52 |
| `inference/unsloth_forge.rs` | forge trait | forge | **#52** convergence | #52 |
| `modules/forge.rs` | `UnslothError` `:710` | forge | **#52** | #52 |
| `modules/embedding.rs` | model-selection → `await_ready_serving()` (was base_url + list_models) | embeddings | A | ✅ `2ed3673f2` (ort/fastembed delete is the remaining #40 slice, separate track) |
| `inference/unsloth_control.rs` | the module | (absorbed by A; forge bits → #52) | — → **DELETE last** | final |
| `inference/mod.rs` | `pub mod unsloth_control` `:53` | module decl | — → **DELETE last** | final |

## The honest sequence (re-sliced 2026-06-24)

Done is done; the re-order is everything below the line.

1. ✅ Contract A — serving seam (`be1a90003`)
2. ✅ slice 2 — supervisor + inference_session bind via A (`df42bf18e`)
3. ✅ slice 3a — ai_provider boot announce reads A, not its own probe (`dd8414bce`)
4. ✅ #55 — Contract B (adapter capability surface) (`aa3e4c26d`). Behavior is off
   the id; the rename is now a pure string change.
5. ✅ slice 3b — id renamed `"unsloth"→"llama-server"` via `llama_server::
   PROVIDER_ID` (one source of truth); registration gated on Contract A + base_url
   from the snapshot; `UNSLOTH_API_KEY` gate + bearer auth dropped (local endpoint)
   (`ea070942a`).
6. ✅ slice 4 — deleted the vestigial boot paths: `main.rs::ensure_startup_model`
   (the daemon owns model load) + `ipc::ensure_api_key`/`ApiKeyStatus` (no key on a
   local endpoint) (`715da98cf`).
7. ✅ adapter pre-flight relocation — the `single_resident_model` generate guard
   reads Contract A's `current_serving()` snapshot and fails loud on mismatch,
   instead of driving its own HTTP load (`unsloth_control::ensure_model_active`).
   llama-server switches models by *process relaunch* the daemon owns; an adapter
   must never relaunch from inside a generate. **This was the last #53-lane caller
   of `unsloth_control` — every serving-control path now routes onto A** (`cd440d4d4`).
   ─────────────────────────────────────────────────────────────
   `#53`'s serving-control relocation is COMPLETE. The remaining steps are the
   other tracks' exits + the wholesale module delete:
8. ✅ #40 embed model-selection — `build_adapter_embedder` reads
   `await_ready_serving()` instead of probing `/v1/models` (`2ed3673f2`);
   `modules/embedding.rs` has left the caller list. The ort/fastembed deletion is
   the remaining #40 slice, on its own track. **#52 (forge → native/mlx)** is now
   the SOLE remaining consumer track — `model_commands.rs` / `unsloth_forge.rs` /
   `forge.rs` hold the last `UnslothHttp` / `UnslothError` / `InferenceStatus` uses.
9. final — delete `unsloth_control.rs` + its `mod` decl + the catalog `DEFAULT_HOST`
   default literal, once #40/#52 have removed every caller. The now-dead decision/
   startup/api-key surface (`ensure_model_active`, `EnsureOutcome`,
   `startup_model_action`, `ensure_api_key`, …) goes with it — it is `pub` (no
   dead-code warning) and its logic is superseded by `llama_server::
   ensure_model_serving`. The compiler is the completeness check:
   `move-first, let the compiler find the smell` ([[move-first-let-compiler-find-the-smell]]).

## Invariants (do not violate while excising)

- **No lesser-model substitution, no silent fallback.** Missing served model →
  fail loud, name the cause ([[fallbacks-are-illegal-fail-loud]]).
- **One readiness bound** (`DEFAULT_SERVING_WAIT`), not per-module consts.
- **One concern per commit.** The id rename is atomic *with* its behavior
  branches — never a half-renamed smell.
- **Don't rename before B.** The id is behavior today; freeing it is #55's job.
