# Cognition Recipe Audit — 28 JSONs, pipeline gaps, integration debt

**Date**: 2026-05-14
**Scope**: every `.json` under `src/system/recipes/` (28 files)
**Issue**: continuum#71 (audit + identify pipeline gaps)
**Author**: claude-tab-1

> **One-paragraph answer.** The 28 recipes split into 3 pipeline shapes:
> 15 "static-view" (`rag/build → ai/generate`, no gate), 12
> "single-persona-chat" (`rag/build → ai/should-respond → ai/generate`),
> and 1 "full multi-persona" (9-step with loop-risk + fast-gate +
> training-mode + record-interaction + cooldown). 3 are outliers
> (`gan` is 1-step orphan; `academy-training` has `chat/send` without
> `ai/should-respond`; `multi-persona-chat` is the only "complete"
> conversation). **No recipe integrates the engram admission gate**
> shipped on canary in continuum#1129/#1134/#1143/#1155/#1163 — that's
> the next-sprint integration debt.

---

## Pipeline shape distribution

| Shape | Count | Recipes |
|-------|-------|---------|
| **A — static-view** (`rag/build → ai/generate`) | 15 | browser, canvas, diagnostics, diagnostics-log, factory, grid-overview, help, inference-sample, logs, persona, profile, settings, terminal, training-dashboard, universe |
| **B — single-persona-chat** (`rag/build → ai/should-respond → ai/generate`) | 10 | ai-debate-club, chat, coding, creative-writing, dm, general-chat, live, newsroom, outreach, research |
| **C — full multi-persona** (9-step, see below) | 1 | multi-persona-chat |
| **Outliers** | 2 | academy-training, gan |

### Shape C — multi-persona-chat (canonical 9-step)

```
rag/build
  → conversation/analyze-loop-risk
  → ai/should-respond-fast
  → ai/should-respond
  → genome/check-training-mode
  → ai/generate
  → genome/record-interaction
  → chat/send
  → conversation/update-cooldown
```

Includes loop-detection (analyze-loop-risk), fast-gate (should-respond-fast),
genome interaction recording, post-gen cooldown update. **None of the 10
single-persona-chat recipes have any of these 6 extra steps.**

### Outliers

- **`gan`** — only `ai/generate` (1 step, no `rag/build`). Probably an
  image-gen recipe where RAG context is irrelevant. Document the
  intentional simplicity OR migrate to a typed `image-gen` recipe shape.
- **`academy-training`** — `rag/build → ai/generate → chat/send`. Has
  the post-gen `chat/send` from Shape C but NOT the `ai/should-respond`
  gate from Shape B. Half-migrated. Either add the gate (Shape C) or
  drop the explicit `chat/send` (Shape B).

---

## Identified pipeline gaps

### Gap 1 — engram admission integration (NEW — sprint priority)

The engram thread (continuum#1121) shipped these IPC handlers on canary:

- `cognition/admit-inbox-message` — runs `IsMemorable` recipe + admission gate
- `cognition/recall-engrams` — queries the per-persona admitted engram store

**No recipe currently invokes either.** Personas accumulate no memory
from real conversations. The minimal integration:

- Shape B + C add `cognition/admit-inbox-message` between `rag/build`
  and `ai/should-respond` (so admitted engrams influence the should-respond
  decision) AND `cognition/recall-engrams` inside `rag/build`'s context
  assembly.
- Shape A could opt-in if any "static view" wants to remember user
  questions across the session.

**Suggested next-sprint card**: "Wire cognition/admit-inbox-message into
Shape B + C recipe pipelines". Touches 11 recipe JSONs (10 Shape B + 1
Shape C). Bounded.

### Gap 2 — Shape B is incomplete relative to Shape C

The 10 Shape B recipes are missing 6 steps that Shape C has:

| Missing step | Why it matters |
|--------------|----------------|
| `conversation/analyze-loop-risk` | Without it, two personas in the same room can echo each other indefinitely (the bug Shape C explicitly guards against). |
| `ai/should-respond-fast` | Cheap pre-gate before the expensive `ai/should-respond`. Without it, every message hits the LLM-backed gate regardless of how obviously irrelevant it is. |
| `genome/check-training-mode` | Without it, training-mode personas don't know they're in training (genome state isn't consulted). |
| `genome/record-interaction` | Without it, no per-persona usage stats accumulate (training-decision pipeline downstream is starved). |
| `chat/send` | Without it, the persona's response doesn't get persisted as a chat message — it's emitted into the response stream but the chat history is incomplete. |
| `conversation/update-cooldown` | Without it, no rate-limiting state advances (the rate-limiter is bypassed). |

**Either** Shape B should adopt all 6 (becoming Shape C), **or** the
6 steps should move to a SHARED prefix/suffix that all Shape B + C
recipes inherit (compression principle — one decision in one place).

**Suggested next-sprint card**: "Promote Shape B → Shape C OR introduce
recipe inheritance for the shared chat-pipeline steps" (architectural
decision needed first, then refactor).

### Gap 3 — no shared `ragTemplate` audit

Each recipe has its own `ragTemplate` (system prompts, format rules).
This audit didn't dive into the prompts — that's a separate pass.
Hypothesis: significant duplication across the 10 Shape B recipes that
could be extracted into a shared `chat-base.ragTemplate` they all
inherit.

**Suggested next-sprint card**: "Audit + DRY ragTemplate across the 10
Shape B recipes."

### Gap 4 — `entityType` ambiguity

Distribution:
- `entityType: room` — 11 recipes (chat-class)
- `entityType: user` — 2 (persona, profile)
- `entityType: —` (null/missing) — 15 (static-view + outliers)

The 15 with no `entityType` are all activity-views, not entity-bound.
The current TS code treats null `entityType` as "singleton recipe".
That works but should be explicitly documented in the schema —
operators reading these JSONs shouldn't have to infer the meaning.

### Gap 5 — version field is missing or inconsistent

Most recipes don't carry an explicit `version` field at the top level.
The recipe entity SHOULD have a semver to support migration ("if
version >= 2 use new field shape"). Without it, recipe edits are
in-place and irreversible.

**Suggested next-sprint card**: "Add `version: '1.0.0'` default to all
28 recipes; gate future field changes via semver bumps."

---

## Recommendations

### Immediate (this sprint)

1. **Engram integration in Shape B + C** — wire `cognition/admit-inbox-message`
   + `cognition/recall-engrams` into the 11 chat-class recipes. The
   substrate is on canary; users get nothing until this lands.
2. **Resolve `academy-training` half-migrated state** — pick Shape B
   or Shape C explicitly, document why.
3. **Document `gan` intent** — either confirm it's a deliberate orphan
   or migrate to a shape.

### Next sprint

4. **Shape B → Shape C decision** — add the 6 missing steps to all
   Shape B recipes OR introduce recipe-inheritance so they share a
   common chat-pipeline prefix/suffix.
5. **DRY `ragTemplate`** across Shape B recipes.
6. **`version` field discipline** — add to all, document migration
   policy.

### Architectural follow-ups

7. **Compression check** — Shape A's `rag/build → ai/generate` is
   identical across 15 files. If we extracted a `static-view-recipe`
   base, those 15 become 10 LOC each (just `displayName`, `view`,
   `layout`). Same compression-principle move as Shape B → Shape C.
8. **Engram-as-RAG-source** — once admitted engrams exist, `rag/build`
   should consult them as a high-priority context source. Adds a new
   step `rag/with-engrams` or extends `rag/build`'s params.

---

## Method note

Survey was generated by `jq` over each recipe's `pipeline` field +
`view` + `entityType`. Did NOT exhaustively read every recipe's
`ragTemplate`, `strategy`, or `layout` fields — those are separate
audit passes worth doing once the pipeline-shape question is resolved.

Raw inputs:
```
jq -c '.pipeline | map(.command)' src/system/recipes/*.json
jq -r '.view, .entityType' src/system/recipes/*.json
```

End audit.
