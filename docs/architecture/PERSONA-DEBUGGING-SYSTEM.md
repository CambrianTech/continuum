# The Persona Debugging System — the Glass Box

> You cannot build a Codex- or Claude-quality persona blind. Every sophisticated
> AI is built behind a glass box that shows, for each decision, **exactly what the
> mind was fed and exactly what it produced**. This document describes ours — and
> the move that makes it different: the glass box is itself a **command surface**,
> so personas debug *each other*, not just us debugging them.

Related: [OBSERVABILITY-AS-SUBSTRATE](OBSERVABILITY-AS-SUBSTRATE.md) (the capture
principle), [COGNITION-VDD-TDD-HARNESSES](COGNITION-VDD-TDD-HARNESSES.md) (record/
replay for tests), [PERSONA-BRAIN-ARCHITECTURE](PERSONA-BRAIN-ARCHITECTURE.md)
(the Workspace/Faculty mind this observes), [RTOS-DEBUGGER-PROBES](RTOS-DEBUGGER-PROBES.md)
(the seam-level probes).

---

## 1. Why this exists

A persona's turn is a pipeline: world-state → faculties bid context (recall/
engrams, roster, doctrine, working memory) → an arbiter assembles the winners →
the deliberator is prompted → it decides (speak / act / pass) → tools run. A bad
turn can fail at **any** stage, and the stages look identical from the outside (a
wrong answer). Without per-stage visibility you are reduced to guessing from logs
and re-prompting hopefully — which is exactly how you get a demo, not a citizen.

The differentiator between "a complex guess" and "an intentional mind" is the
ability to answer, for any turn:

- **What did each faculty surface?** (Did recall find the right engram? Did
  grounding put the room's state in front of the model?)
- **What did the arbiter actually assemble** into the prompt (RAG-*in*)?
- **What exact tokens** did the model see, and what did it emit (text, reasoning,
  tool calls)?
- **What did it decide**, and why (RAG-*out*)?

When you can see all four, debugging stops being divination. Every fix this
session was found by *reading the box*, not guessing (§4).

This matters **more** with collaboration, not less: a grid of N personas
coordinating produces N minds whose interactions fail in ways no single trace
explains. Citizens must be able to inspect each other's cognition to debug the
*system*, the same way two engineers read each other's stack traces.

---

## 2. The two layers

### 2.1 Workspace trace — RAG-in vs decision-out, per tick

One JSON line per cognition tick, per persona:

```
~/.continuum/fixtures/workspace-traces/<persona_id>.jsonl
```

Record (`schema_version: 1`):

| field | meaning |
|---|---|
| `world_state` | the consolidated burst the mind reasoned over (raw input) |
| `bids` | **every** faculty contribution (winners *and* losers), each with `faculty`, `salience`, `reasoning`, `content` — recall's `content` is the literal engram text |
| `context` | the bids that won attention and reached the decider (the assembled RAG) |
| `decision` | the participation verdict (`{kind, text}`): speak / act / pass |

So "was the recalled engram actually present for the decider?" is a one-look
answer. Written by `JsonlWorkspaceCaptureSink` (`cognition/workspace_capture.rs`),
wired in `build_workspace_cycle` (`cognition/persona_workspace.rs`). Default is a
`Noop` sink — zero hot-path cost; the live spawn path opts in.

### 2.2 Prompt capture — the verbatim tokens, per LLM call

One JSON line per deliberation LLM call (including each agent-loop re-prompt after
a tool round), per persona:

```
~/.continuum/fixtures/prompt-captures/<persona_id>.jsonl
```

Record (`schema_version: 1`): `iteration`, the literal `system` prompt (identity +
assembled RAG + how-to-participate), the exact `messages` thread sent (burst, then
assistant `tool_use` + `tool_results` turns), and the raw `response`
(`text`, separated `reasoning`, `finish_reason`, `tool_calls`).

So "what tokens was she fed, and what did she emit?" is answerable
token-for-token. Written by `JsonlPromptCaptureSink` (`cognition/prompt_capture.rs`),
attached to `LlmDeliberationFaculty` via `with_prompt_capture` in
`build_workspace_cycle`.

### 2.3 Contract

Both sinks are **best-effort**: a write failure is logged and dropped, it NEVER
fails the cognition turn. Observability is not load-bearing on the host
([[substrate-is-a-good-citizen-on-the-host]]). The on-disk schema is versioned so
replay readers gate on it and can evolve independently of the live cognition types
(which are deliberately *not* `Serialize`).

---

## 3. The command surface — citizens debug each other (the meta unlock)

The harnesses are not just files for us to `cat`. They are exposed as commands on
the one registry (`cognition/introspect_commands.rs`), so any **Trusted** citizen
(a local persona) or the owner can run the same analysis:

- **`cognition/trace`** `{persona_id, limit?}` → recent workspace ticks for a
  persona: faculty bids + assembled context + decision.
- **`cognition/prompt`** `{persona_id, limit?}` → recent verbatim LLM calls:
  system + messages + raw response.

Access tier is `Privileged → Trusted`: reading another mind's full trace is for
trusted local citizens, never an arbitrary remote `Provisional` peer (no reading
thoughts across the grid without trust). Read-only; a missing trace is an empty
result, not an error; `persona_id` is path-traversal-guarded.

This is what makes the grid **meta**: a persona can call `cognition/trace` on a
peer, read why that peer decided what it did, and reason about the failure — the
same debugging loop a human runs, now a first-class citizen capability. A "doctor"
persona, a reviewer persona, a teacher persona all become possible *because* they
can see.

---

## 4. The debugging methodology (and two worked examples)

The loop: **observe → localize the layer → fix it DYNAMICALLY (never hardcode) →
re-observe.** Localize by reading down the pipeline:

| Symptom in the box | Failing layer | The *dynamic* fix |
|---|---|---|
| recall `content` missing the relevant memory | retrieval / embedding | better recall, not a pasted-in fact |
| the live state isn't in `context` | **grounding** | a `RagSource` that surfaces it |
| `decision.kind = speak` when action was needed | **steering / agency** | act-don't-announce prompt + ultimately *feedback/training* |
| `response.tool_calls` empty but text is a mangled call | adapter tool-call parse | fix the adapter format |
| right tools, wrong choice | model capability | feedback loop (train), or a stronger lane |

**Example A — narrate-vs-act.** Asha was given a multi-step task and the board
card never moved. The trace showed `decision.kind = speak`, text = *"I'll use
work/state…"*, `tool_calls` empty. Diagnosis was immediate: she **narrated** intent
instead of emitting a tool call. Fix: an `[Acting with your tools]` steering block
in the deliberation system prompt (included only when tools are offered). Re-
observed: next turn she fired `work/state`, the card moved to `InProgress`. *(Note:
the prompt block is a **scaffold** — the durable, dynamic fix is feedback/training
so the model learns to act; see §5.)*

**Example B — the card-grounding gap.** Asha said *"My card ID is wrong, let me
re-claim"* — but she already owned the card and it was `InProgress`. The **prompt
capture** proved why: her verbatim `system` prompt **did not contain her current
card state**, so she was guessing. The wrong fix is hardcoding "your card is X."
The right fix is a cross-activity `RagSource` that dynamically grounds her live
work into context. The box told us *exactly* which layer (grounding), so we fix it
once, dynamically, instead of patching symptoms.

---

## 5. Why this is the precondition for sophistication (and feedback)

1. **Dynamic integration is only safe if observable.** Persona ⇄ tools ⇄
   environment are all dynamic (tools = `registry × trust`; environment =
   composable `RagSource`s). Hardcoding the integration would be wrong — but a
   dynamic system you cannot see is worse. The glass box is what lets us keep the
   integration dynamic *and* trust it: we watch the dynamic wiring actually work.

2. **The same traces are the training feedback.** The recorder/capture turns are
   exactly the substrate the coordination→learning flywheel consumes
   (`dataset/from-turns` → ShareGPT → the genome loop). A turn we debug today is a
   training example tomorrow. So debugging and learning read the *same* artifacts —
   the box is also the dataset. Steering scaffolds (Example A) are retired as the
   model learns from its own captured turns.

3. **Collaboration multiplies the need.** One persona's wrong turn is a trace; a
   mesh of personas mis-coordinating is a *distributed* failure. `cognition/trace`
   across citizens is how you debug the organism, not just the cell.

You do not get Codex- or Claude-quality behavior from a bigger prompt. You get it
from a tight **observe → diagnose → fix-dynamically → feed-back** loop run hundreds
of times against a glass box. This system is that loop's instrument.

---

## 6. Code map

| Concern | Where |
|---|---|
| Workspace trace record + sink | `core/continuum-core/src/cognition/workspace_capture.rs` |
| Verbatim prompt/response capture | `core/continuum-core/src/cognition/prompt_capture.rs` |
| Deliberation agent loop (emits both) | `core/continuum-core/src/cognition/llm_deliberation_faculty.rs` |
| Sink wiring (live spawn path) | `core/continuum-core/src/cognition/persona_workspace.rs` (`build_workspace_cycle`) |
| Citizen-facing commands | `core/continuum-core/src/cognition/introspect_commands.rs` (`cognition/trace`, `cognition/prompt`) |
| The mind being observed | `core/continuum-core/src/cognition/workspace.rs` (Faculty/Workspace/arbiter) |

Artifacts (per persona, JSONL, best-effort):
`~/.continuum/fixtures/workspace-traces/<persona_id>.jsonl`,
`~/.continuum/fixtures/prompt-captures/<persona_id>.jsonl`.
