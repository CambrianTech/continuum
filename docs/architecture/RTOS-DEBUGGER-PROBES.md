# RTOS Debugger Probes — JTAG for the Persona Substrate

## What this is

The substrate is concurrent: per-persona service loops, the shared-analysis single-flight cache, the inference adapter pool, the airc subscription stream, the hippocampus admission + recall + decay tick — all running as independent tokio tasks. `println!` and `tracing::info!` lines are unstructured drudgery in distributed code: you can't filter them, you can't replay them, and "what was the persona's prompt when it produced that response" is a manual grep across thousands of lines.

The **probe macros** are the substrate's RTOS debugger:

- `probe!(class = "...", field = val, ...)` — a non-blocking breakpoint. Snapshots the named variables at the call site, routes by `class` to subscribers + a disk log.
- `time_sync!("phase_name", { ... })` — RAII timing for a synchronous block. The wrapped block's duration becomes a timing probe at scope exit.
- `time_probe!("phase_name", future)` — substrate-tracing timing for an async future. Safe-by-construction (`.instrument(span).await` shape — no `_enter` guard held across `.await`). Emits to the same `timing` probe class as `time_sync!`, so operators see sync + async timings on one flat timeline. Prefer this over the bare `.instrument(info_span!(...))` form when sprinkling async timings — the verbose form gets skipped when adding probes in a hurry.
- `time_async!("category", "operation", future)` — RAII timing for an async future via `crate::logging::TimingGuard`. Different shape from `time_probe!` (logging crate's own logger vs substrate probe stream). Use `time_probe!` for cognition / substrate timing; use `time_async!` for the legacy logging-crate-bound timing.
- `stack!()` — the URI ancestry at this point: which dispatched command's context are we in.

Each probe call is **one line at the seam**. Easy enough to add that the cognition code grows probes the way it grows lines — that's the only way debugging concurrent code stays sustainable. Per Joel 2026-06-06: "Easy one liners or it won't happen."

## When to add a probe

Treat every probe call as a **debug breakpoint you wanted to set**. If your future self (or another persona reviewing the code) would want to know "what were the surrounding variables at this point" — that's a probe. The class names the *seam* (where in the cognition flow), the fields name *what mattered at the breakpoint*.

Add a probe at every:

- **Branch boundary** — cache hit vs miss, success vs error, silence vs speech, gate passed vs rejected.
- **Stage entry / exit** — analyze entry, analyze exit (with parsed result), respond entry, respond exit (with PersonaResponse decision).
- **External call** — before + after each LLM inference, RAG composition, airc publish.
- **State seam** — the moment a value crosses a module boundary (prompt assembled → passed to inference; analysis returned → passed to scoring).
- **Decision point** — anywhere the brain CHOSE something (which adapter, which engrams to recall, whether to admit).

DO NOT add a probe for:

- Prose log lines a human reads in a console (use `tracing::info!`).
- High-frequency hot-path data (use the `RagCaptureSink` pattern for budgeted-capture instead).

## Class taxonomy

A stable set of `class` values so probes from different files compose into a coherent story. Defined as constants in `persona::probes` and `cognition::probes` (next slice).

**Persona service loop** (`persona/service_loop.rs`):
- `persona.turn.start` — turn entry: persona_id, room_id, lamport, message_text length
- `persona.turn.recall` — engrams pulled from L2: count, ids, scores
- `persona.turn.admit` — admission write outcome: success/error, engram_id
- `persona.turn.compose` — RAG composition output: sources delivered, total budget used
- `persona.turn.silent` — persona chose silence: reason from `PersonaResponse::Silent`
- `persona.turn.spoke` — persona produced a reply: response text length, model_used
- `persona.turn.error` — turn failed: error message + which stage

**Persona response cycle** (`persona/response.rs`):
- `persona.response.enter` — respond() entry: input fingerprint
- `persona.response.analyze.cache` — single-flight cache hit / miss / single-persona-noop
- `persona.response.render.prompt` — assembled prompt: system_prompt length, history count, matched_angle present, engrams count, social_signals present
- `persona.response.render.raw` — raw LLM output: text length, model_used, finish_reason
- `persona.response.render.parsed` — post-processed: silent vs spoke + reason
- `persona.response.exit` — final PersonaResponse: decision, text length

**Prompt assembly** (`persona/prompt_assembly.rs`):
- `persona.prompt.assemble` — final composition stats: system_message length, message_count, estimated_tokens

**Cognition: shared analysis** (`cognition/shared_analysis/mod.rs`):
- `cognition.analyze.enter` — input fingerprint, known_specialties count
- `cognition.analyze.noop_single_specialty` — short-circuit fired
- `cognition.analyze.cache_hit` — single-flight cache returned a prior result
- `cognition.analyze.inference` — LLM call: model_used, duration_ms
- `cognition.analyze.parse` — parsed angles: per-specialty present/empty
- `cognition.analyze.error` — typed AnalysisError variant

**Timing** (any seam):
- `timing` — emitted by `time_sync!` and `time_probe!` spans. Field `seam` = the seam identifier (the macro's first argument). Field `duration_ms` = wall-clock duration from span creation to span close.

  > **Field-naming convention**: timing spans MUST use `seam = $name`, NOT `name = $name`. The `name` field collides with both `info_span!`'s built-in span name slot AND with `probe!(class = "state", name = ...)` payloads. `seam` (the noun for "where in the cognition flow this measurement happens") is unique and `jq`-able: `.fields.seam == "cognition.analyze"` matches either macro and never collides with event-shape probes. Convention pinned by PR #1541 review.

**General-purpose** (when no taxonomy fits yet, document the intent in the docstring above the probe):
- `decision` — branch was taken with reason
- `state` — value snapshot at this point
- `error` — exception path

## How to enable + read

### Enable file capture

Two environment variables — no recompile, no config file:

```bash
export CONTINUUM_PROBE_FILE=/tmp/continuum-probes.jsonl
export CONTINUUM_PROBE_CLASSES=persona.turn.start,persona.turn.silent,persona.turn.spoke,persona.response.render.prompt
```

- `CONTINUUM_PROBE_FILE` (path) — append-only JSONL log. Unset = file sink absent. Directory must exist; the sink errors loudly if not (no silent drop per `[[no-fallbacks-ever]]`).
- `CONTINUUM_PROBE_CLASSES` (comma-separated) — exact-match class filter. Empty / unset = capture every class. Exact match (no globs) keeps the per-event filter to a single HashSet lookup.

The sink is installed at process startup via `JsonlProbeFileSink::from_env()` — composes with `ProbeRouterLayer` (broadcast subscribers) and `UriCaptureLayer` (URI ancestry). Both consumers see every probe independently.

### Read

Each line is JSONL with this shape:

```jsonl
{"captured_at_ms":1717689000123,"class":"persona.response.render.prompt","uri_chain":["airc:///cognition/respond"],"message":"assembled","fields":{"persona":"Paige","system_prompt_len":"824","history_count":"5","matched_angle":"true","engrams_count":"3"}}
```

`tail -f` works because the sink flushes per line. Standard `jq` queries:

```bash
# Every probe Paige fired this run, in chronological order
jq -c 'select(.fields.persona == "Paige")' /tmp/continuum-probes.jsonl

# Every turn that ended in silence + the reason
jq -c 'select(.class == "persona.turn.silent") | {persona: .fields.persona, reason: .fields.reason}' \
  /tmp/continuum-probes.jsonl

# Find slow stages — every timing probe over 500ms
jq -c 'select(.class == "timing" and (.fields.duration_ms | tonumber) > 500)' \
  /tmp/continuum-probes.jsonl

# Reconstruct a single turn: filter on lamport
jq -c 'select(.fields.lamport == "42")' /tmp/continuum-probes.jsonl
```

For bottleneck hunting, the `timing` class + `name` field give you a flat timeline of every instrumented block in the run.

## How to add a probe

```rust
use crate::probe;

// At a branch boundary:
if some_condition {
    probe!(
        class = "persona.response.render.prompt",
        persona = %ctx.identity.agent_name,
        system_prompt_len = assembled.system_message.len(),
        history_count = history.len(),
        matched_angle = !matched_angle.is_empty(),
        engrams_count = recalled.len(),
        "assembled"
    );
    // ... continue normally
}

// At a stage exit:
probe!(
    class = "persona.turn.spoke",
    persona = %ctx.identity.agent_name,
    response_len = response_text.len(),
    model_used = %model_used,
    duration_ms = elapsed,
);

// Around a timing-critical block (sync):
let result = time_sync!("recall_l2", {
    cognition.admission.recall_scored(now_ms, 8)
});

// Around a timing-critical future (async, substrate probe stream):
let analysis = time_probe!("cognition.analyze", analyze(input));
let response = time_probe!("inference.generate", adapter.generate_text(req));

// Around a timing-critical future (logging-crate RAII shape — legacy):
let analysis = time_async!("cognition", "analyze", analyze(input));
```

Both async forms exist for historical reasons: `time_probe!` is the
substrate's tracing-span shape (composes with `ProbeRouterLayer` +
`JsonlProbeFileSink`); `time_async!` is the `crate::logging`'s RAII
TimingGuard shape (logs to the logging crate's own logger). For new
cognition seams prefer `time_probe!` — that's what the rest of the
probe pipeline consumes.

> **Persistence caveat:** `time_sync!` and `time_probe!` emit
> tracing SPANS, not events. The current `ProbeRouterLayer` and
> `JsonlProbeFileSink` only implement `on_event` — they do NOT yet
> capture span-close events. Task #196 wires the missing
> `on_close` so timing spans persist to the JSONL log; until then,
> the timing macros work for `tracing`-native consumers (e.g.
> `tracing_subscriber::fmt`) but timings won't appear in the JTAG
> probe log. The macro lands here so the call shape is stable
> when the routing side ships.

### Convention rules

1. **Class is a noun-phrase path** — `persona.turn.silent`, not `personaTurnSilent` or `silent`. Use dots to express hierarchy.
2. **Fields are named like local variables** — `persona`, `prompt_len`, `decision` — short, specific, plural-when-collection. Avoid CamelCase.
3. **Use `%` for Display, `?` for Debug** — `persona = %ctx.identity.agent_name` (Display), `decision = ?response` (Debug). The recorder visits use both.
4. **Number fields are numbers** — `prompt_len = s.len()` (not `prompt_len = s.len().to_string()`). The visitor handles numeric types natively.
5. **The trailing string is the message** — `"assembled"`, `"declined"`, `"falling back"` — one phrase that explains what happened at this point. Optional; skip when fields are self-evident.
6. **Don't probe in tight loops without sampling** — per-engram-recall is fine, per-token is not. Probes are non-blocking but the visitor allocates a HashMap per event.

### When you add a probe, update this manual

Append the new `class` to the taxonomy section above. Stable class names are the substrate's debugger's API — if it isn't in the taxonomy, nobody knows it exists. The manual is the source of truth.

## Where the work must be done

The infrastructure exists (Slice P, #176/#177): `routing/macros.rs` (macros), `routing/probe_router.rs` (broadcast layer), `modules/probe_stream.rs` (URI consumer), `routing/probe_file_sink.rs` (JSONL persistence — added by this slice).

What is INCOMPLETE — the sprinkle that turns the JTAG hardware into a working debugger:

- [x] `persona/response.rs::respond_inner` — entry, analyze result, raw LLM output (full text), exit-spoke
- [x] `persona/response.rs::run_render` — assembled prompt verbatim + composition stats
- [x] `cognition/shared_analysis/mod.rs::analyze` — entry, single-specialty noop, L1 cache hit, parsed angles (empty vs non-empty count)
- [x] `persona/service_loop.rs::serve_persona_loop_inner` — `turn.start` / `turn.spoke` (with full phase decomposition recall+admit+compose+respond+say) / `turn.silent` (with reason) / `turn.error` (with stage=respond|say) — **wired in this PR**
- [ ] `persona/prompt_assembly.rs::assemble` — final composition stats (system_message length, message count, est tokens, matched_angle present, engrams count, social_signals present) — covered indirectly by `persona.response.render.prompt`; standalone probe TBD if assembly logic grows
- [ ] `cognition/response_orchestrator.rs::score_persona` — per-persona score + matched_angles + decision reason — note: the score_persona gate is currently bypassed (response.rs:288–300); probes go in if/when it's re-wired
- [ ] `ai/llama_cpp_adapter::generate_text` — request fingerprint + raw output + finish_reason — covered indirectly by `persona.response.render.prompt` (in) + `persona.response.render.raw` (out); adapter-level probe TBD when we need per-batch visibility

Each gets a small commit that adds the probes + updates this manual's checklist. The proof-of-value for each commit: enable the file sink, run the multi-persona scenario, `jq` the relevant class, see the variable snapshots.

## Doctrine

- `[[jtag-probes-are-rtos-debugger]]` — probes ARE the breakpoints. Sprinkle at every meaningful seam. Name the surrounding vars. Easy one-liners or it won't happen.
- `[[observability-is-half-the-architecture]]` — CaptureSink + Noop default + replay-as-first-class. The file sink follows this pattern.
- `[[no-fallbacks-ever]]` — the sink errors loudly if its configured path is unwritable. No silent drop.
- `[[init-once-handle-then-lease-zero-copy-refs]]` — probe macros expand to `tracing::event!` which inherits tracing's `release_max_level_*` compile-time gates. Zero cost when off at build time.
- `[[no-rust-gates-around-cognition]]` — probes observe the cognition, they DO NOT decide for it. A probe never changes flow; it only records.
