<!-- Plan line 16 (be the addendum), the Serving seam of REPO-GENOME-AND-COURSES.md §17.
     Joel, 2026-10-27: "let's build a better system that all these systems can call." -->

# The driver gateway: one door every agent can call

## Why

The agents people use already exist: Claude Code, Codex CLI, Grok Build (xAI's agent, Apache 2.0 since July 2026, read-only reference at `~/.continuum/reference/grok-build`), Hermes and opencode. Each one re-sends its whole conversation on every step to a model behind an API. Each one keeps what it learns as notes reread into context: Grok Build's `xai-grok-memory` is markdown plus a consolidation pass, the same mechanism as Claude's MEMORY.md.

On 2026-10-27 that mechanism visibly failed. A correction already written in notes ("one owner per resource") was repeated within an afternoon.

Continuum can give these agents two things a stock endpoint cannot:
- **Speed.** Each session's KV stays warm across steps and pages instead of being re-prefilled. A stock server keeps one LRU slot cache, so an agent loses its prefix whenever another session runs.
- **Learning.** Every exchange is perception for the user's genes. A correction then lands in weights that persist across restarts, not in a note.

The agent changes nothing but its base URL.

## What exists today (canary 107af9167)

- `continuum-core/src/http/` serves only the Anthropic `POST /v1/messages`, on a dynamic port, started on demand for Claude Code sentinels (`mod.rs`, `anthropic_compat.rs`, 442 + 262 lines).
- Requests carry no session identity, so a call is anonymous transient traffic. On a busy node that means it borrows a resident's slot (`borrow_slot_for_transient`) and evicts her.
- There is no OpenAI surface. An agent could only reach llama-server directly, which bypasses paging, scheduling and learning.

## Design: one gateway, wire formats as adapters

**One module: `Gateway`.** It is a `ServiceModule` in the canonical shape and the only door for external agents. It folds in and deletes `http/mod.rs`'s Anthropic-only server.

The port is stable and configured, never dynamic, because an agent needs a URL it can keep. The listener is loopback by default; a LAN or grid listener needs a bearer token.

`continuum gateway env <agent>` prints exactly what to set:
- `ANTHROPIC_BASE_URL` for Claude Code;
- `OPENAI_BASE_URL` for Hermes, opencode and Codex;
- `--xai-api-base-url` for Grok Build.

**Wire formats are adapters holding only the diff.** Each one is a decoder into the one internal request (`TextGenerationRequest`) and an encoder out of its stream, with SSE included. A field the agent sent is read whole or the call is refused before anything is generated (`continuum-gateway` `wire/decode.rs`): a malformed tool or a provider's built-in one, a token limit outside the engine's range, a stop sequence that is not a string. Nothing an agent declared is dropped.

| Wire format | Path | Agents |
|---|---|---|
| Anthropic Messages | `POST /v1/messages` | Claude Code |
| OpenAI Chat Completions | `POST /v1/chat/completions` | Hermes, opencode, most others |
| OpenAI Responses | `POST /v1/responses` (incl. `previous_response_id`) | Grok Build, Codex CLI |
| Discovery | `GET /v1/models` | all |

Tools pass straight through. The agent executes its own tools; the gateway never does. Tool-call parsing and rendering stays the engine's one template owner (SERVING-ENGINE §1 `template`).

**A session is a mind.** Each agent session maps to a driver mind, `PageKey::driver(user, session)`.
- Session memory is opt-in (`x-continuum-session: on`) until G2 seats driver minds through `MindMemory::seat`: switched on by default, a driver session queued behind resident pins, a 61 s steady-state step against 18 s without (card e3ce65f9).
- The session id comes from the protocol when it carries one: `previous_response_id`, an `x-session-id` header, or OpenAI's `user` field.
- Otherwise it is the hash of the conversation's stable head: the system prompt plus the first user message.

A driver mind is seated by `MindMemory::seat` (SERVING-ENGINE §1a) exactly like a resident:
- it keeps its warm KV and pages to RAM or disk;
- it is never anonymous transient traffic, so it never evicts a resident unsaved;
- it is scheduled by the one scheduler (plan goal 9) by its activity temperature, so a busy agent and four residents share the engine fairly.

The `Responses` API's server-side state is exactly this: the response id names the mind's frontier.

**Learning (ships at plan step 4, wired now).**
- Every exchange is captured by the existing recorder, with request, tool calls, results and reply, into the same perception stream residents use.
- The corrections are the user's next message, a rejected edit, or a reverted change (§17).
- Genes are selected by what the session is on, and one genome serves drivers and residents.
- `.continuum/scope` decides per repo what may be learned from.
- Credentials and secrets never enter perception, captures or genes.
- A private session trains only on the user's own nodes.

**What the gateway does not do.** It does not run an agent loop, grade work, or rewrite the agent's prompt. It is the model behind the agent, with memory and learning.

## One glass box for every system (Joel, 2026-10-27)

"We should have built our replay and glass boxing around this … so that this is straightforward regardless of system."

**The seam.** Every step of every agent (Claude Code, Codex, Grok Build, Hermes, opencode) crosses the gateway. Every act of every resident crosses the deliberation faculty. Both write ONE capture, so everything we built for residents works on any system unchanged:
- the act inspector: per-act prefill, decode, reuse, view layers and divergence;
- the replay;
- the glass-box playback.

**What moves.** The capture owner (`continuum-cognition/src/cognition/prompt_capture.rs`, 940 lines, schema v4, `fixtures/prompt-captures/<persona>.jsonl`) moves down, below both callers: Serving's floor, `continuum-engine` or its own small crate.
- It is keyed by mind (`PageKey`): resident, fork, or `driver(user, session)`.
- The deliberation faculty and the gateway are its two writers, by the same `submitted`/`completed` records.
- The private-room withholding rule moves with it.
- Scope rules (§17) decide what a driver session may record at all.

**The layering: learning and the genome ride the same seam** (Joel: "learning and genome must work anyway. All about proper layering").

| Layer | Owners | Rule |
|---|---|---|
| 1. Serving floor | the engine; `MindMemory` (SERVING-ENGINE §1a); the capture owner | One identity, the mind (`PageKey`), for a resident, a fork or a driver session. |
| 2. Writers | the residents' deliberation faculty; the gateway (every external agent) | Both write the same capture. Neither knows the readers. |
| 3. Readers of the capture | glass box and replay (act inspector, playback, `serving-replay`); Dreaming (`dataset/from_captures` → training → genes) | They read records, never which system produced them. |
| 4. Genome | genes selected by what a mind is on, served at `MindMemory::seat` | A gene change calls `forget`, so KV computed under the old genome is never restored. |

A driver session is perception like a resident's act. It is learned from under the scope rules and served genes the same way. Nothing above layer 1 special-cases a system.

**Why it is the tuning loop.**
- A captured Claude Code or Codex session replays against our engine on the dyno, with no live agent and no residents, under any engine config. Their loop becomes our validator: prefill per step, reuse, and time to a passing test.
- A resident's captured act is read by the same inspector beside an agent's step on the same task, so the gap is visible act by act, not argued: context size, layers, act count, reasoning length.
- Tuning our system to match theirs is then a replay comparison, and glass-boxing any system that calls us is free.

## Build order (each slice lands with its validator first, and deletes what it replaces)

| # | Slice | Validator (seconds, no residents) | Live gate | Owner |
|---|---|---|---|---|
| G0 | `Gateway` module on a stable port. The Anthropic adapter moved into it; `http/mod.rs`'s server deleted. | Claude Code's captured requests decode to the same `TextGenerationRequest` as today. | Claude Code on the M5 answers through it. | Fable |
| G1 | OpenAI Chat Completions and Responses adapters, SSE. `GET /v1/models`. | Round-trip fixtures from each agent's captured traffic (Grok Build's `mock_server_tests` show its shapes). | Hermes, opencode and Grok Build each complete a coding task on the M5. | Fable |
| G1.5 | **One capture for every system:** `prompt_capture` moved below cognition, keyed by mind. The gateway writes the same schema-v4 records as residents. | The act inspector and `serving-replay` read a captured Claude Code session exactly as a resident's. | The four-agent median-bug run glass-boxed beside Benchy's run on the same task. | Fable |
| G-lean | **A lean install for drivers** (Joel: "only install these features, reducing bloat"). A `continuum-serve` binary built from the Serving seam only: gateway, `continuum-serving` (the engine lifecycle), `continuum-engine` with `MindMemory`, and the capture owner. Learning is opt-in by adding `continuum-dreaming`. No personas, rooms, airc, desktop or academy. The provider registration moves down from the core's AI module, so both binaries share it. | `cargo tree` shows no `continuum-core`, `continuum-mind` or `continuum-cognition`. The four-agent run passes on the lean binary. | Binary size, build time and RSS at idle against the full core. The same four agents on a machine with only the lean install. | Fable (binary), Codex (engine lifecycle seam) |
| G2 | Session identity: `PageKey::driver`, seated through `MindMemory::seat` (needs §1a step 1). | Replay a captured agent session: prefix reuse per step at least 90%, and no resident evicted unsaved. | **The speed gate:** the same agent and model on the gateway vs stock llama-server, with four residents running. Time per agent step and re-prefilled tokens. | Fable (seat), Codex (scheduling temperature) |
| G3 | Capture to the perception stream, under the scope rules. | A captured session yields perception rows and no secret. | — | Cormac (learning side) |
| G4 | Driver genes: selected by what the session is on, served to the session. | — | **The learning gate** (§17's proof): correct an agent once, restart, give it an unseen related task. It succeeds; without the gene it fails. | Cormac, Fable |
| G5 | **Org genomes** (Joel, 2026-10-27: "genome is also given to them; shared … learned repos shared within an org"). A repo's gene is learned from every developer's agent sessions in that repo, plus the repo's history (REPO-GENOME-AND-COURSES §17). Every org member's agent wears it the first time it works there. Private repos' genes stay on the org's own nodes over the grid; publishing beyond the org is an explicit act; secrets never enter. Fitness is usage across the org; revocation removes the skill. | A gene learned from one member's corrected session passes the acceptance test on another member's agent in the same repo. | An org of two (two machines, two agents, one repo): a correction made on one machine is applied on an unseen task by the other's agent after a restart. | Cormac (genome), Codex (grid trust), Fable (gateway serving) |

G0 and G1 can start now. G2 waits on MindMemory's `seat`, and G4 on the residents passing the learning acceptance test (plan pipeline step 3).

## Crate divisions this work exposes (Joel: "if you see more crate divisions note or build them")

| Crate | State | From | Why it is its own crate |
|---|---|---|---|
| `continuum-gateway` | BUILT 10-27 | `continuum-core/src/http` (Anthropic-only endpoint deleted) | The Serving seam's door; depends only on the engine and capture. |
| `continuum-capture` | BUILT 10-27 | `continuum-cognition`: `prompt_capture`, its store and owner, `mind_room`, `fixture_dir` (1,920 lines moved with history, 38 callers) | One capture and one privacy gate below every writer (faculty, gateway) and reader (inspector, replay, Dreaming). |
| `continuum-mind-memory` | NEXT | `continuum-engine/src/inference/slots/*` plus the paging block of `turn_admission.rs` | SERVING-ENGINE §1a's owner. A crate keeps every other path from reaching `holders` or the ledger again. |
| `continuum-providers` | NOTED | the AI provider module's registration in `continuum-core` | The lean `continuum-serve` and the full core must register adapters the same way, without the core. |
| `continuum-serve` (bin) | NOTED | — | G-lean: the Serving seam alone, learning opt-in. |
| `*-types` crates | NOTED | wire types shared by two crates (for example `TextGenerationRequest` and the capture records) | Grok Build's pattern (`xai-grok-config-types`): consumers depend on types, not on the engine. Cut where a reader such as `serving-replay` or the act inspector needs records without the engine. |
| `wire_tap` | NOTED | `continuum-engine/src/inference/wire_tap.rs` | The raw-stream tap is armed only by the capture lease: it may belong in `continuum-capture`, behind a trait the engine calls. |

## Measured before and after

- Per agent step: submit-to-first-token, total time, tokens re-prefilled, and tokens appended.
- Per node: residents' acts per hour with N driver sessions running. Drivers must never starve residents, and residents must never starve drivers.
- Per gene: the acceptance test above, and the same task without the gene.
