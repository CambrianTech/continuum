# Typed-Observation Refactor Spec — Killing the Stringly Act→Observe Channel

> Blueprint produced by the `typed-observation-refactor-design` workflow (6 Opus agents,
> 2026-08-09). Map phase read every seam; design phase synthesized this. Execute against it
> as one gigantic, dependency-ordered, VDD-gated sweep. **Read
> `docs/architecture/PERSONA-COGNITION-PIPELINE.md` before touching the brain.**

**Bug this fixes (verified, run-18057-f1):** a persona's tool RESULT does not reliably
re-enter her next LLM prompt. The result flows as an untyped `String` routed as a perception
*bid* through an arbiter that can silently evict it; a dozen predicates then re-parse that
rendered prose. She grepped once, the result never threaded back, she yielded → 0-byte patch.
Structural, not cognition. (Supersedes the disproved fold-clip theory: `ContextBudget::unknown()`
keeps the payload whole — the drop is arbiter eviction of the result bid, not truncation.)

**Guard:** this is the guarded cognition brain. This refactor threads a **typed value** through
the existing act→observe→re-perceive contract. It does **not** introduce a
`will_respond`/`response_text` chatbot shape, does not rewrite the cognitive contract, and does
not collapse the recall vs recency split.

**One-line thesis:** the typed values already exist (`ai::types::ToolCall{id,name,input}` +
`ai::types::ToolResult{tool_use_id,content,is_error}`, correlated inside `NativeBatchOutcome`),
and `WmKind` already exists to end reparse — but `apply_act` flattens them to `format!` prose at
`apply.rs:327-333`, discards `ToolCall.id` (correlates positionally), and every downstream
consumer re-derives structure from the rendered string. We thread the typed pair end-to-end and
delete the reparse.

---

## 1. Type Definitions

New module: **`core/continuum-core/src/cognition/act_observe/observation.rs`**. All types derive
`Serialize, Deserialize` (they land in `VolatileSnapshot` → `~/.continuum/personas/<id>/volatile.json`,
also the grid-sync wire format) and `#[ts(export)]` (`ToolCall`/`ToolResult` already cross the
Rust→TS boundary, so a struct built from them must too — ts-rs law, CLAUDE.md).

```rust
use crate::ai::types::{ToolCall, ToolResult};
use std::path::PathBuf;
use ts_rs::TS;

/// Semantic class of a tool verb. Produced ONCE from ToolCall.name at the act
/// seam; every predicate that used to grep the rendered receipt for a verb
/// prefix now reads this. Single home of the verb→class mapping duplicated as
/// (a) the `wrote` bool at apply.rs:480-483, (b) the "I ran code/write(" scans
/// in perception.rs, (c) the orientation-prefix scans in is_redundant_orientation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ToolVerb.ts")]
pub enum ToolVerb {
    Write, Edit, Apply, Commit,        // mutate disk
    Run, Shell, Read, Screenshot,      // observe the world
    ListCommands, Help, Tree, Search,  // orient
    Other,
}

impl ToolVerb {
    /// Normalizes `_`→`/` first (models emit both forms — same as apply.rs:481).
    pub fn classify(name: &str) -> Self {
        let n = name.replace('_', "/");
        // exact SUBSTRING semantics preserved from apply.rs:480-483 so `wrote` is unchanged.
        if n.contains("write")  { return ToolVerb::Write; }
        if n.contains("edit")   { return ToolVerb::Edit; }
        if n.contains("apply")  { return ToolVerb::Apply; }
        if n.contains("commit") { return ToolVerb::Commit; }
        match n.as_str() {
            "commands/list" => ToolVerb::ListCommands,
            "commands/help" => ToolVerb::Help,
            "code/tree"     => ToolVerb::Tree,
            "code/search"   => ToolVerb::Search,
            "code/run"      => ToolVerb::Run,
            "code/shell"    => ToolVerb::Shell,
            "code/read"     => ToolVerb::Read,
            "interface/screenshot" => ToolVerb::Screenshot,
            _ => ToolVerb::Other,
        }
    }
    pub fn mutates(&self) -> bool { matches!(self, ToolVerb::Write | ToolVerb::Edit | ToolVerb::Apply | ToolVerb::Commit) }   // replaces mutated_workspace / `wrote` bool
    pub fn observes(&self) -> bool { matches!(self, ToolVerb::Run | ToolVerb::Shell | ToolVerb::Read | ToolVerb::Screenshot) } // replaces wrote_without_observation's obs-verb class
    pub fn is_orientation(&self) -> bool { matches!(self, ToolVerb::ListCommands | ToolVerb::Help | ToolVerb::Tree) }          // replaces is_redundant_orientation prefix scan
}

/// The typed payload of ONE tool call's result. REPLACES the format! blob.
/// `result` reuses the existing ai::types::ToolResult verbatim (tool_use_id == ToolCall.id).
/// `verb`/`paths` PRECOMPUTED at the act seam so no consumer re-derives from prose.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ToolOutput.ts")]
pub struct ToolOutput {
    pub result: ToolResult,               // single source of the raw payload; correlated by tool_use_id == call.id
    pub verb: ToolVerb,                    // computed once via ToolVerb::classify(call.name)
    #[ts(type = "Array<string>")]
    pub paths: Vec<PathBuf>,               // files touched, from call.input — exact membership, immune to head-truncation
}

/// Per-call outcome. Flattens the FIVE return sites of the old Option<String>.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ActStatus.ts")]
pub enum ActStatus {
    Executed,
    Errored { message: String },              // executor Err — apply.rs:273 currently only warn's + drops this
    AlreadySatisfied { repeat: usize },       // apply.rs:136-161 short-circuit
    RedundantOrientation { repeat: usize },   // apply.rs:174-197 short-circuit
}

/// ONE act = typed pair (call, output) + status. `call` retains ToolCall (INCLUDING .id)
/// so correlation is by id, not by outcome.results.get(i) positional index (apply.rs:297).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/Observation.ts")]
pub struct Observation { pub call: ToolCall, pub output: ToolOutput, pub status: ActStatus }

/// The BATCH result of apply_act — replaces `Option<String>`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/ActOutcome.ts")]
pub enum ActOutcome {
    NoHands,                                                   // was: None at apply.rs:56
    ExecutorError { calls: Vec<ToolCall>, message: String },   // was: None at apply.rs:273 — now distinguishable from no-hands
    Acted { acts: Vec<Observation> },                          // was: Some(observation) at apply.rs:160/196/495
}
impl ActOutcome {
    /// The Some/None signal settle_step used (settle.rs:386-388) becomes typed.
    pub fn produced_an_act(&self) -> bool { matches!(self, ActOutcome::Acted { acts } if !acts.is_empty()) }
}
```

### Rendering — pure functions, byte-stable (the #205 KV invariant)

The two current inline renderings (`observation` full-recency at `apply.rs:327-333`,
`recall_observation` collapsed via `render_act_for_recall` at `apply.rs:334-340`) become **pure
functions of `Observation`** (re-render byte-identical) in `observation.rs`, keeping the existing
helper bodies (`summarize_args_for_recency`, `humanize_result_content`, `bound_recency_result`,
`render_act_for_recall`) unchanged:

```rust
impl Observation {
    pub fn render_recency(&self, budget: &ContextBudget) -> String { /* moved from apply.rs:327-333 */ }
    pub fn render_recall(&self) -> String { /* moved from recency.rs:186-215 render_act_for_recall */ }
}
```

Every field earns its place from a seam (justification table): `Observation.call` retains
`ToolCall.id` (kills positional correlation); `ToolOutput.result` reuses executor's `ToolResult`
from `NativeBatchOutcome`; `verb` is the `wrote` computation kept instead of thrown away and makes
the dead "I ran code/write(" predicates un-driftable; `paths` is exact typed membership immune to
receipt head-truncation; `ActStatus`/`ActOutcome` un-flatten the five `Option<String>` return sites
(the return String is currently a dead data path — the caller reads only Some/None).

---

## 2. Ordered Edit Plan (strict dependency order)

`[MOTION]` = pure code-motion/type-plumbing, behavior identical, green at every step.
`[BEHAVIOR]` = observable change, guarded by a test in the same step.

- **Step 0 [MOTION]** — new `observation.rs` with the types; move `render_act_for_recall` out of
  `recency.rs:186-215` into `impl Observation::render_recall` (keep body); `mod observation;` in
  `act_observe/mod.rs`. No caller changes. `cargo check` green.
- **Step 1 [MOTION]** — add `fn extract_paths(input: &Value) -> Vec<PathBuf>` (file_path/path/paths);
  `ToolVerb::classify` subsumes the `wrote` logic at apply.rs:480-483 (leave apply.rs:480 for now).
- **Step 2 [BEHAVIOR]** — `apply_act` builds `Vec<Observation>` and returns `ActOutcome`:
  - apply.rs:50-55 signature `-> Option<String>` → `-> ActOutcome`.
  - apply.rs:56 `cycle.acting()?` → `let Some(body) = … else { return ActOutcome::NoHands };` (#1).
  - apply.rs:136-161 already-satisfied → `status: AlreadySatisfied{repeat}`, keep nudge fact,
    `return Acted{acts}` (#2). apply.rs:174-197 redundant-orientation → `RedundantOrientation` (#3).
  - apply.rs:259-275 executor Err → `return ExecutorError{ calls, message }` (#4).
  - apply.rs:290-341 per-call loop → also push `Observation{ call.clone(), ToolOutput{ result:
    outcome.results.get(i)…, verb: classify(&call.name), paths: extract_paths(&call.input) }, status }`.
    Keep the two renderings for now (still feed record_receipt/engram).
  - apply.rs:495 `Some(observation)` → `Acted{acts}`.
  - settle.rs:381-390 sole caller → `o.produced_an_act() => Acted, NoHands|ExecutorError => ActUnfulfilled`
    (behavior identical; ExecutorError now distinguishable for a future backstop).
  - Guard: mod.rs:773-817 `the_hands_change_the_mind`, mod.rs:832 `identical_already_satisfied…` green.
- **Step 3 [BEHAVIOR]** — WorkingMemory stores the typed act on the Receipt:
  - working_memory.rs:277-283 `WmEntry` gains `#[serde(default)] pub act: Option<Observation>`
    (back-compat, same pattern as saved_at_ms/build_sha). `text` STAYS (byte-stable `recent()`).
  - `record_receipt_typed(&self, acts: &[Observation], rendered: &str)` stores `act` alongside the
    `[action #{seq}] {head}` text; rendered string produced ONCE by `render_recency` at this boundary.
  - apply.rs:464 → `record_receipt_typed(&acts, &observation)`; engram admit (apply.rs:412-431) still
    consumes `render_recall()` — recall channel stays separate (#166).
  - add `recent_acts() -> Vec<&Observation>`, `active_act() -> Option<&Observation>`.
  - Guard: snapshot/restore round-trip loads an OLD (no `act`) volatile.json without panic.
- **Step 4 [BEHAVIOR, one predicate per commit]** — migrate predicates off `recent(): Vec<String>`
  (safest first): entries_since_last_settlement→`WmKind::Settlement`; has_real_action_receipt→`has_receipt()`;
  is_redundant_orientation→`verb.is_orientation()`; mutated_workspace→`recent_acts().any(verb.mutates())`
  (preserve concern-window off-by-one); wrote_without_observation→typed; claimed_file_without_act→
  `paths` exact match; all_calls_already_satisfied→`loop_fingerprint()` equality (severs render coupling);
  latest_action_seq→`WmKind::Receipt{n}.max()`; fact tags→`enum FactKind on WmKind::Fact`. **This is
  where the seam-5 live drift dies** — `[no-deliverable]` stops false-firing because mutated_workspace is typed.
- **Step 5 [MOTION]** — `render_recency` is the ONLY recency-string producer, called at the
  record_receipt_typed boundary; engram uses render_recall; remove dead inline format! in apply.rs:327-340.
- **Step 6 [BEHAVIOR, the 18057 fix]** — thread the typed tool_use/tool_result PAIR into the message builder:
  - llm_deliberation_faculty.rs:1431-1439 — for the live act (`active_act()`), emit the typed pair
    via the existing zero-caller ctors (ai/types.rs:617/636/657): `assistant_tool_use(call)` +
    `tool_result(ToolResult)` bound by tool_use_id == call.id; **trailing/append-only** (#205), produced
    deterministically so live heartbeat + eval drive_to_settle build byte-identical turns.
  - **Silent-drop fix:** route this pair as a **durable per-turn record**, NOT a perception bid at
    `WORKING_MEMORY_SALIENCE` that `arbiter.focus()` (workspace.rs:~1846) can evict. The just-executed
    act's result is pinned for the turn it re-enters — can no longer be outbid and dropped. This is the
    structural cause of the 18057 0-byte patch.
  - Guard: the §4 VDD gate (`the_hands_change_the_mind` as a typed assertion).

---

## 3. Predicate-Deletion List

Every string-match dies for a typed read (full table): `starts_with(WM_SETTLEMENT_PREFIX)`→
`kind==Settlement`; `format!+trace.contains(sig)`→`loop_fingerprint()` equality; `contains("commands/list(")…`
→`verb.is_orientation()`; `match_indices("[action #")`→`matches!(kind, Receipt{..})`/`has_receipt()`;
`contains("I ran code/write(")` ×3→`verb.mutates()`; `contains("code/run(")…`→`verb.observes()`;
`l.contains(file_token)`→`paths.iter().any(==claimed)`; `contains("[unfulfilled]"…)`→`Fact{FactKind::…}`;
`split("[action #").max()`→`Receipt{n}.max()`; `strip_prefix("[action #")` (recall_faculty.rs:341-347)→typed body;
`text.lines().next()` (perception_facts.rs:181-200)→`recent_acts()` typed head. **`calls_signature`/
`loop_fingerprint` (settle.rs:58) is the ONE consumer already typed — it stays as the model everything converges to.**

---

## 4. VDD Gate Order

Build after Step 0 then every [BEHAVIOR] step:
`export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"; cargo check -p continuum-core --lib --features metal,accelerate`.
Escalate to `cargo test …` after Steps 2/3/4. `df -h /` each cycle; sweep ghost `core/target` if free < 20 GB.

Tests MIGRATE (string fixtures → typed) in lockstep with §3 — notably perception.rs:259-365 fixtures
hand-build `"I ran code/edit(...)"` strings **the live path never emits** (the GREEN≠REACHABLE defect):
rebuild them as typed `Observation{ verb: Edit, paths:[x.py] }`. mod.rs:832 must stay green through Step 4.7.

**18057 replay assertion (the acceptance gate — convert `the_hands_change_the_mind`, mod.rs:773-817):**
scripted settle loop via mod.rs `ScriptedExecutor`: tick1 `Act(grep "needle")` → `ToolResult{tool_use_id==call.id,
content:"match at foo.rs:42"}`; tick2 assert the tick-2 perception carries the content BY READING THE TYPED FIELD
`active_act().output.result.content.contains("match at foo.rs:42")` AND `active_act().call.id ==
active_act().output.result.tool_use_id` (NOT `perceived.split("[action #")`); and assert the message builder emitted
the assistant_tool_use↔tool_result pair (same tool_use_id, trailing) that arbiter capacity pressure does NOT drop.
This is act-grained (neither workspace_capture v2 nor turn_replay can assert it — which is why run-18057-f1's
3-line capture couldn't prove the drop). Optionally extend workspace_capture to v3 with `act_receipt: Option<Observation>`.

---

## 5. Risks + Invariants to Preserve

1. **KV byte-stability (#205):** `WmEntry.text` rendered once at record time; append-only, `#seq`-stamped;
   `render_recency` MUST be pure (store `text` alongside `act`, don't re-render at read). Step 6 pair is trailing.
2. **Loop detection / #206:** `loop_fingerprint()` is the single durable repeat counter; the escalating warning +
   orientation counter are honest proprioception, never a steer. `all_calls_already_satisfied`→fingerprint-equality
   must still be true for a byte-identical re-issue, false for a corrected re-write (mod.rs:832).
3. **Proprioception facts (`[unfulfilled]`/`[confabulation]`/`[unverified]`):** migrating to `Fact{FactKind}` must
   fire on the SAME conditions — EXCEPT `mutated_workspace` currently mis-fires `[no-deliverable]` on successful writes
   and can't fire `[unobserved]`; the typed verb **fixes** these, so their behavior CORRECTS. A fact is honesty not an
   actuator — correcting the fact firing changes what the settle GATES see, which is the point.
4. **Recall vs recency split (#166):** two channels — `render_recency` (full → record_receipt, prompt) vs
   `render_recall` (collapsed → EngramOrigin::Tool, semantic-recall-GATED-OUT). Do NOT collapse. `last_action` full-result
   + settlement-gated `active_from_seq` is a THIRD separate recency channel — don't fold into `entries`.
5. **Index-space trap:** `fg_calls` is a FILTERED subset of `calls` (long-running stripped to background). The
   observation loop iterates `fg_calls`; the probe/`wrote` iterates ORIGINAL `calls`. Correlating by
   `tool_use_id == call.id` removes the positional hazard; keep the two iteration domains distinct where they differ.
6. **`NativeBatchOutcome.media`/`stored_ids`** ignored today — out of scope; `ToolOutput` is extensible via `#[serde(default)]`.
7. **VolatileSnapshot / grid-sync back-compat:** every new serde field gets `#[serde(default)]`; old volatile.json restores.
8. **One test mod per file; typed fixtures REPLACE the dead string ones** (don't leave `"I ran code/edit("` alive
   beside typed); each migrated fixture carries `// what this catches:` naming run-18057-f1 / #158 / #206.

**Discipline:** `observation.rs` is a new <500-line file owning the types + both renderers; `apply.rs` shrinks as the
two inline format! blocks move out. Green at every step; behavior change only where a test pins it. Refactor as a QE.
