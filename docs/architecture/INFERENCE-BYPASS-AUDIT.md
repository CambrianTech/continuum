# Inference Bypass Audit (task #105)

> Every consumer that needs a model response in-process MUST call
> through `Commands.execute('inference/llm/request', ...)` OR the
> `InferenceHandleStore` (the in-process equivalent). Direct calls
> to `adapter.generate_text(...)` from consumer code bypass the
> substrate's lane lifecycle, observability sinks, and pressure
> response.
>
> This document enumerates every direct `adapter.generate_text(...)`
> call site in the tree (as of 2026-05-31) and classifies each as
> canonical or bypass.

**Status:** Audit complete (2026-05-31). Confirmed 6 bypasses + 4
canonical paths. Follow-up tasks queued per bypass.

**Parents:**
- [`AI-COMMAND-NAMESPACE.md`](AI-COMMAND-NAMESPACE.md)
- [`INFERENCE-SCHEDULING-AND-SCARCITY.md`](INFERENCE-SCHEDULING-AND-SCARCITY.md)
- [`INFERENCE-LANES-REALISTIC.md`](INFERENCE-LANES-REALISTIC.md)

---

## Doctrine

From [[inference-is-an-adapter-always-in-the-loop]]:

> "Every consumer that needs a model response in-process —
> `rag_inspect`, the eventual `PromptAssembly` + turn loop,
> prompt-replay tools, training fixtures, **persona service cycles**,
> sentinel adversarial review, training/eval harnesses — calls
> through `Commands.execute('inference/llm/request', { persona_id,
> prompt, ... })`. Never builds its own `LlamaContext::generate(...)`."

The boundary:
- **Canonical** — direct `adapter.generate_text(...)` calls that ARE
  the command-handler implementation OR the handle-store
  implementation. These can't go through themselves; they're the
  primitive.
- **Bypass** — direct calls from consumer code (cognition turn
  loops, persona responses, sentinel work, HTTP handlers) that
  should route through the command surface so they get
  observability, lane lifecycle, and pressure response for free.

---

## Canonical paths (allowed)

These IMPLEMENT the inference command surface; the adapter call is
the primitive.

### `inference/llm_module_service.rs:381` — `run_adapter_inference`

Inside the `inference/llm/request` command handler. THE canonical
inference command. Adapter call is correct.

### `inference/handle_store.rs:312` — `InferenceSession::generate`

Inside `InferenceHandleStore::generate`. The canonical in-process
wrapper of the inference command. All in-process Rust callers
SHOULD route through this; calling `adapter.generate_text` here IS
the implementation.

### `modules/ai_provider.rs:802, 1036` — AIProviderModule command handlers

These ARE the legacy command surface (pre-handle-store). Canonical
for the routes they implement. As of [[INFERENCE-LANES-REALISTIC.md]]
the modern surface is `ai/inference/*` commands → handle store;
the AIProviderModule legacy paths should eventually migrate but for
now they're the command handler itself, not a bypass.

### `ai/heuristic_adapter.rs:*` (test code)

`#[cfg(test)]` adapter unit tests. The adapter's own contract is
the subject of test; direct calls are the right pattern.

### `inference/handle_store.rs:592` (test code)

Same — handle store's own tests.

---

## Bypasses (need follow-up refactors)

These six call sites are in consumer code and should route through
`InferenceHandleStore::generate` (in-process) or
`Commands.execute('inference/llm/request', ...)` (cross-process).

### #B1 — `cognition/generate_response.rs:300`

**The biggest bypass.** This is the persona response generation
path — the doctrine explicitly names it:

> "persona service cycles, sentinel adversarial review, training/eval
> harnesses — calls through `Commands.execute('inference/llm/request', ...)`"

Currently calls `adapter.generate_text(inference_request)` directly
after resolving the adapter from `global_registry`. Should:
1. Open a handle via `InferenceHandleStore` (or
   `ai/inference/open` for cross-process scenarios).
2. Generate against the handle (the lane lifecycle attributes the
   KV cache to the persona automatically).
3. Close on completion (or hold for the persona's service cycle).

**Priority:** Highest. This is the substrate's hot path for persona
turns and inherits NO lane benefits today.

**Follow-up task:** #105-B1.

### #B2 — `persona/response.rs:506`

The persona's "raw render" — calls
`adapter.generate_text(request)` directly to convert assembled
prompt → text. This is a parallel implementation to
`cognition/generate_response.rs` (older path). Same fix:
route through handle store / command surface.

**Priority:** Highest. Same hot path as #B1.

**Follow-up task:** #105-B2.

### #B3 — `cognition/should_respond.rs:235`

The "should I respond?" gating call — small, cheap, high-frequency
(every inbox message). Routes through the registry directly.

Going through the handle store gives:
- Observability (capture sinks see the gating decision flow)
- Lane attribution (the gating call counts against the persona's
  budget like any other inference)

**Priority:** Medium. High frequency means high value; but the call
is short + cheap so the observability benefit is the main win.

**Follow-up task:** #105-B3.

### #B4 — `cognition/validate_response.rs:196`

Sentinel-style response validation — quality check on a generated
response. Same shape as #B3: direct registry lookup + direct
adapter call.

**Priority:** Medium. Lower frequency than gating but explicitly
named in the doctrine ("sentinel adversarial review").

**Follow-up task:** #105-B4.

### #B5 — `modules/agent.rs:656`

Agent module's generate path (IPC bridge entry). Currently sets
`persona_id: None` (not persona-owned). Still should route through
the inference command for observability + pressure response.

**Priority:** Medium. Lower frequency than persona turns; less
critical since not persona-attributed.

**Follow-up task:** #105-B5.

### #B6 — `http/mod.rs:233`

HTTP "local coding agent" endpoint. External-API-shaped entry
point. Could argue this IS at the command-surface layer (the HTTP
endpoint is the cross-process boundary), but routing through
`Commands.execute('inference/llm/request', ...)` would add the
substrate's standard observability + pressure response for free.

**Priority:** Lower. The HTTP path is an external boundary; the
substrate's lane benefits matter less to one-shot external callers.

**Follow-up task:** #105-B6.

---

## Debatable

### `persona/rag_inspect.rs:434` — `run_inference_probe`

The chained inspection probe (task #104) calls
`adapter.generate_text(request)` directly inside the library
function. Justification:

- It's a one-shot probe for INSPECTION, not a persona service
  cycle.
- Opening + closing a handle for a single shot adds overhead
  without behavioral benefit (handle lifecycle matters for
  multi-call sessions).
- Going through the handle store would mean: probe opens its own
  handle, generates, closes. Functionally identical, more
  ceremony.

**Verdict:** Acceptable as-is. If the inspection ever grows into
multi-turn probing (replay-against-multiple-models), revisit.

**No follow-up task.**

---

## Audit method

```bash
grep -rn "\.generate_text(" /Users/joel/Development/continuum/src/workers/continuum-core/src/ \
  | grep -v "/tests/"
```

19 hits total; classified above. Re-run this grep before merging
any new inference-using code and add to this doc if new call sites
appear.

---

## Follow-up plan

Bypass fixes land as separate focused commits (one or two per
commit; not a big-bang refactor). Order by priority:

1. **#B1 (`generate_response`) + #B2 (`response`) together** —
   they're duplicate hot paths for persona turns. Refactor both to
   route through the handle store in one slice. Single PR.

2. **#B3 + #B4 together** — `should_respond` and `validate_response`
   are sibling gating/validation calls; same shape, refactor both
   in one slice.

3. **#B5 — agent module** — independent slice.

4. **#B6 — HTTP endpoint** — independent slice. Lowest priority.

Each refactor:
- Open a handle (or reuse the persona's existing handle if it's a
  lane-owning session).
- Generate against the handle.
- Close (or release back to the lane's session loop).
- Wire the persona_id through so the lane's
  observability + footprint accounting work.

Tests use the heuristic adapter; coordinator + handle store are
already in tree (#107, #109) so the refactor is composition.

---

## When to update this doc

Re-run the grep + update this doc:

1. Before any PR that adds a new `adapter.generate_text` call site.
2. As part of every #B-prefixed follow-up task that fixes one of
   the bypasses (remove the entry from §Bypasses and move it to
   §Canonical or just drop it).
3. As part of a quarterly substrate-architecture audit.

The point is structural visibility — if a new caller appears that
this doc doesn't classify, it's a sign the substrate's command
surface isn't being used as intended.
