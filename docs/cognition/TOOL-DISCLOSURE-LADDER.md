# The Tool Disclosure Ladder

*How a citizen's verbs reach her prompt — one ladder, three rungs, no rung repeating another.*

Status: design, 2026-08-16. Measured against build 4743 (`340eb6c34`).
Supersedes the "shrink the surface" framing on #333; feeds #163 (tool conformance
harness) and unblocks the #2332 ratchet.

---

## 1. What it costs today, measured

Not estimated. `describe_tool_tokens` serializes each `NativeToolSpec` with
`serde_json` and charges `len/3 + 8`; this table replicates that projection exactly
against the running core's own descriptors.

| slice | tokens | share |
|---|---:|---:|
| `properties` (JSON Schema) | 3,658 | 53% |
| `description` (prose) | 1,986 | 29% |
| `definitions` (`$ref` targets) | 219 | 3% |
| per-tool template margin (26 × 8) | 208 | 3% |
| **native payload** | **6,935** | |
| framing floor (rest of the 8,979 fixed cost) | ~2,044 | |

**26 verbs, mean 266 tokens each.** Top five: `activity/spawn` 871,
`work/list` 672, `perception/look` 561, `code/edit` 476, `code/run` 428.

### The claim this retracts

An earlier pass on this asserted the surface ran "3–5× over a clean design," on a
calibration of 50–150 tokens per well-formed tool. **That calibration was fiction.**
The tools a frontier agent runtime actually ships — a shell verb, a subagent
launcher, a workflow orchestrator — run several hundred to well over a thousand
tokens each, because a rich description is what teaches correct use. At 266/verb
Continuum's native schemas are *leaner than the surface the comparison was drawn
from*.

Two more hypotheses died the same way, and are recorded so nobody re-derives them:

- **`$schema` / `title` boilerplate.** Already stripped —
  `persona_tools::tool_input_schema_from` carries only `type` / `properties` /
  `required` / `definitions`. **0 tokens.** (The `$schema` line is visible in
  `commands/list` output, which is a *different* surface. Reading one surface's
  output and attributing its shape to another is the error here.)
- **Multi-paragraph doc rationale.** Real, but **550 tokens across 2 commands** —
  8%, not a general lever.

**The schemas are not where the waste is.** What follows is therefore not a diet.

---

## 2. The two real defects

Joel, 2026-08-16: *"So many are just devoid of knowledge or repetition."* Both are
measurable, and both are quality defects rather than size defects.

### 2.1 Devoid — a field with no description

`code/edit` offers 12 properties. Eight of them — `search`, `replace`, `line`,
`new_content`, `start_line`, `end_line`, `content`, `all` — ship as bare
`{"default": null, "type": ["string","null"]}`. No description. They are flattened
duplicates of the fields already inside `edit_mode`'s tagged object, present
because the parser is deliberately forgiving about both shapes.

An undescribed nullable field is **worse than an absent one**: it is a named slot
with no stated purpose and no stated validity, which is an invitation to fill it
with something plausible. That is #334 (*"a persona with nothing to write invents a
`file_path`, and THAT field degenerates"*) arriving through a different door.

Being forgiving at the PARSER is correct. Advertising both shapes in the SCHEMA is
not — the schema is a teaching surface, and it should teach exactly one way in.

### 2.2 Repetition — the surface is composed twice

Both of these reach the prompt every turn:

- 26 full `NativeToolSpec` schemas (6,935 tokens), and
- the prose `render_tool_menu` over the authorized catalog.

The menu re-states in prose what the schemas already carry in structure. This is
#333, now confirmed by reading `compose_system` rather than asserted.

Worth recording because it explains how it happened: the field doc on
`LlmDeliberationFaculty::native_specs` still describes it as *"the DISCOVERY PAIR —
`commands/list` + `commands/help`"*. That was true when the menu was the only
schema-bearing surface. It stopped being true when commands began declaring
`NATIVE` individually, and no comment marked the transition. The duplication was
never designed; it accreted, one justified `NATIVE` flag at a time.

---

## 3. The design

The ladder already exists — three rungs are built and working. The defect is that
the top rung **bypasses** the ladder instead of sitting on it.

```
rung 0  SPINE       every category, every verb NAME              always
rung 1  EXPANDED    verbs + one-line summaries, per category     situational
rung 2  SCHEMA      full argument schema for one verb            on demand / working set
```

**Nothing is hidden at rung 0.** Every verb she has is named every turn. This is
the "not censoring them" line: discovery never depends on guessing that a verb
exists. What varies is how much *detail* rides along, and detail is what costs.

### 3.1 The keystone: `NATIVE` becomes situational, not static

Today `NATIVE` is a `const bool` on each command — a static list of 26 that only
grows, which is precisely why the agentic-surface ratchet keeps needing to be
re-pinned. Each addition is individually justified (#339's lifecycle write-backs,
#358's `room/members`, the room-membership trio) and collectively unbounded.

The move: **rung 2 is this turn's working set, not a fixed list.** The same
`expanded_categories` signal that already drives rung 1 selects which verbs get
full schemas. Consequences:

- Adding a verb costs a **NAME** (a few tokens at rung 0), not a **SCHEMA** (266).
  The ratchet stops being a tax on giving citizens new capabilities.
- The payload becomes a function of the turn. A citizen editing code carries
  `code/*` schemas; a citizen triaging the board carries `work/*`. Neither carries
  both.
- `commands/help` stays permanently at rung 2 so any named verb is one call from
  its full schema — the escape hatch that makes selection safe.

### 3.2 The safety line this must not cross

`rebuild_tool_surface` carries a deleted-code warning worth honoring: the #206
shrink cliff amputated the tool surface to fit a budget and produced a model that
could not act at all, flipping 10/10 ↔ 0/6 on one token of window.

**Situational selection is not a budget clamp.** The distinction is load-bearing:

- *Situational* — chosen by what she is doing. Deterministic given the turn.
  Degrades to "the wrong category expanded", recoverable in one `commands/help`.
- *Budget clamp* — chosen by what fits. Non-deterministic w.r.t. the task, and
  degrades to "no hands", unrecoverable within the turn.

If a working set does not fit the served window, that is a **measured demand
reported to the governor** (the existing `min_window_for_agentic_surface` sensor),
never a silent amputation.

### 3.3 The quality bar for a tool definition

Both defects in §2 are conformance failures, and there is already a card for
enforcing conformance by construction (#163). This design contributes the rules:

1. **Every parameter carries a description.** No exceptions — an undescribed param
   is a defect, not a terse one.
2. **No parameter is an alternate encoding of another parameter.** If the parser
   accepts two shapes, the schema advertises one. Forgiveness lives at the parser.
3. **The schema describes the WIRE, for the caller.** Maintainer rationale — why
   the Rust type is what it is, what the doc used to say, why a list is omitted —
   belongs in the file, not in the projection. `schemars` lifts `///` verbatim, so
   this needs a mechanical split, not discipline (see slice 1).
4. **Descriptions teach use, not existence.** "Optional note describing the change"
   is a label. "…recorded in the change history" is the part that tells her when to
   bother. Length is not the enemy; contentless length is.

---

## 4. Build plan

Sliced so each lands and is verifiable alone.

**Slice 1 — the projection splits the audiences.** In `tool_input_schema_from`,
take only the leading paragraph of each field description into the schema;
everything after the first blank line is maintainer-facing. One place, zero
per-command edits, and it matches how the docs are already written. Measured
recovery: **550 tokens**, 310 of them on `activity/spawn`. *This is the "SHRUNK
FIRST" branch the ratchet's own contract asks for, and it is what unblocks #2332
without rationing a verb.*

**Slice 2 — `code/edit` advertises one shape.** Drop the eight flattened aliases
from the schema; keep the parser forgiving. −171 tokens, and it closes a #334-class
invitation on the most-used editing verb.

**Slice 3 — conformance test.** Every native descriptor: every param described, no
param a duplicate encoding. Fails on a new violation. This is the #163 gate applied
to the surface that exists today. *Do this before slice 4 — it is the regression
net for everything after.*

**Slice 4 — rung 2 becomes the working set.** `NATIVE` stops being a static const
and becomes selection over `expanded_categories`. `commands/help` pinned always-on.
The ratchet ceiling then guards the SPINE, which grows sub-linearly in verb count.

**Slice 5 — delete the duplicate.** With rung 2 situational, decide which surface
teaches call form and remove the other. Closes #333.

---

## 5. Why this is a benchmark item

The surface is 6,935 tokens. On a 16k-window lane that is **43% of everything she
has**; on a frontier window it is ~3%. The identical code is either crushing or
invisible depending only on what the lane serves — which is why #332's
16,384 → 26,368 moved more than any schema edit could.

So the benchmark argument is not "the tools are too big." It is:

- **Every token rung 2 spends on a verb she is not using is a token the TASK does
  not get.** On a small lane that trade is the whole margin.
- **A verb that arrives devoid of knowledge gets called wrong**, and a wrong call
  costs a whole act out of a bounded budget — far more than the tokens the missing
  description would have cost.
- **The ratchet currently taxes capability.** Every new verb a citizen needs makes
  the window arithmetic worse for every citizen, so the pressure is to withhold
  verbs. Situational disclosure removes that pressure, which is the difference
  between a citizen and a managed resource.

---

## 6. Reproducing the measurement

The numbers above come from replicating `describe_tool_tokens` against the running
core's own `commands/list` output — same projection (`tool_input_schema_from`),
same estimator (`len/3`), same per-tool margin (8). Re-run it after any slice to
confirm the recovery rather than assume it.

Native verbs are those declaring `const NATIVE: bool = true`; 26 as of build 4743.
