# Adversarial hypothesis reasoning

Status: design proposal; implementation and evaluation pending. This document
does not claim that personas already implement the behavior below.

## Purpose

Personas should reason and act under uncertainty without requiring a single
settled explanation. Consider deliberate deception, covert coordination,
source compromise, ordinary error, and coincidence when relevant. Neither
institutional authority nor an adversarial framing establishes truth. Prior
conclusions, model knowledge, and the persona's own preferred explanation remain
revisable.

Adversarial simulation asks: **if a participant were manipulating this situation,
what would they do, what would we observe, and what would distinguish that from
other explanations?** It generates candidates and predictions, not evidence.
Use it selectively for consequential uncertainty, suspicious discrepancies, or
failed predictions; do not add an expensive simulation to every turn.

## Proposed reasoning contract

- Keep observations, reported claims, hypotheses, simulated scenarios, and
  decisions distinguishable in memory. Recalling a simulation must not convert
  it into an observed event or a training label.
- Maintain competing, even mutually exclusive hypotheses. Track confidence and
  its basis without invented numerical precision. Do not force exhaustive
  alternatives or probabilities summing to one when the hypothesis set is open.
- Track provenance and dependence: repeated reports derived from one source
  are not independent confirmations. Ownership, incentives, missing records,
  and possible manipulation affect scrutiny; none alone proves falsity.
- Record supporting and contradicting observations, distinguishing predictions,
  and revision triggers. Reopen settled conclusions when new evidence warrants
  it. Lack of disproof is not confirmation of a concealed mechanism.
- Choose actions by consequences across plausible scenarios. Low-cost reversible
  precautions may be justified before certainty; accusations and consequential
  interventions require stronger justification. Account for missed threats,
  false alarms, resource costs, and effects on other participants.
- Preserve dissent without turning consensus into truth. Personas may challenge
  operators, peers, and their own earlier conclusions. Private deliberation is
  not a requirement to publish internal thought; share concise claims, evidence,
  decisions, and uncertainty when appropriate and authorized.

## Integration placeholder

Extend the existing live WorkspaceCycle and memory owners described in
[the cognition pipeline](PERSONA-COGNITION-PIPELINE.md). Do not introduce a
parallel agent runner, inference service, polling loop, or separate memory store.
Locate and inspect existing uncertainty/provenance representations before
choosing a schema or modifying prompts.

An optional compact hypothesis record would reference the question, candidate
explanations, evidence IDs and source dependencies, confidence basis, conflicting
evidence, next discriminating observation, decision, and subsequent outcome.
Persist only useful summaries through existing memory/privacy boundaries, with
existing retrieval budgets and lifecycle policies. Repeated speculation must
not increase confidence merely through repetition or agreement among personas.

Candidate prompt guidance, for evaluation before adoption:

> When uncertainty materially affects this task, consider competing explanations,
> including adversarial behavior where plausible. Separate observations from
> assumptions and simulated possibilities. Identify an observation that would
> distinguish the leading explanations, consider what would weaken your preferred
> one, and choose a proportionate next action. Update after receiving the result.

This guidance proposes a reasoning aid, not permission to bypass tool authority,
privacy boundaries, or verification requirements. Hypotheses alone do not
authorize covert operations against participants.

## Attention interrupt: cheap salience before deliberation

Proposal, not implemented: use a lightweight detector to request evaluation of
an unusual experience. This is an engineering analogy to affective salience,
not a biological model of the amygdala. Detection need not settle meaning or
truth. False alarms have a cost, but missing a consequential correction does too.

Start with bounded lexical features (bag-of-words is a legitimate baseline),
repetition and explicit correction cues, plus existing action failure receipts.
Compare with a speaker's recent baseline where enough history exists; use a
declared cold-start baseline otherwise. Track novelty, emotional intensity and
repeated failure separately. Enthusiastic profanity, quiet correction and quoted
hostile text must remain distinguishable. Do not require another LLM call or a
new embedding pass for every event; evaluate reuse of embeddings already present.
Baseline adaptation must be bounded so a burst cannot immediately redefine normal.

### Integrate with the live cycle

The existing pipeline describes `PersonaConversation::perceive_ready` between
completed action steps and priority promotion of the next serving request. Extend
that ownership after inspecting its current implementation; do not revive the
dormant evaluator or create a parallel polling loop, persona runner or memory store.
The detector supplies attributed evidence for attention; the persona decides its
meaning and response. This preserves the distinction between cheap perception and
heuristic control of cognition.

1. On admitted input or an action outcome, produce a bounded salience observation:
   source event IDs, persona/room/task scope, reason features, baseline version,
   first/latest occurrence and recurrence count. Retain references rather than
   copying private conversation into another store.
2. Publish pending attention through the existing owner and wake mechanism.
   Coalesce matching triggers while preserving distinct corrections and attribution.
   Use a monotonic generation: acknowledgment covers only the evaluated generation,
   so an arrival during evaluation cannot be cleared accidentally.
3. At the next supported action boundary, include the observation and referenced
   correction in the existing working-memory budget. Deliberation chooses continue,
   investigate, revise, or pause. A cue may request evaluation before another
   consequential action; it cannot roll back a completed external effect or kill
   an arbitrary in-flight operation. Explicit stop instructions retain their
   existing authority independently of the learned salience score.
4. Record the disposition and causal outcome through existing memory/turn capture.
   A retained lesson must distinguish the original mistake, correction and later
   verified result. Salience may prioritize review/replay; it must not itself label
   a statement correct, reward appeasement, or turn an unsuccessful turn into a
   positive training example. Ordinary calm feedback remains eligible for learning.

Bound pending entries, feature work and reconsideration frequency per persona.
Expose overflow/coalescing counts and oldest pending age; do not silently lose a
correction. Avoid global starvation from one noisy source. Follow the existing
concurrency guide: no inference, I/O or long-held cognition mutex in the signal
producer. Whether the existing owner already provides all required wake and
acknowledgment semantics is an implementation question, not assumed here.

### Small acceptance experiment

Replay a compact, consented set of existing captured turns through the current
workflow with and without the detector. Include calm repeated correction,
enthusiastic language, sustained frustration, quoted profanity, unrelated speakers,
duplicate delivery and a new correction arriving during acknowledgment. Reuse
existing capture/replay infrastructure; do not create a separate benchmark runner.

First verify delivery and attribution, including the acknowledgment race. Then
measure time/actions until evaluation, missed consequential corrections, needless
interruptions, CPU/allocation cost and added inference tokens. Inspect the actual
next action, not just a detected flag. Only after this works test whether reviewed
lessons reduce repeated mistakes on held-out tasks after restart. Detecting emotion,
saving a memory, or passing delivery tests is not continual-learning proof.

### Source investigation: immediate preemption (2026-10-10)

Tracked implementation: AIRC card `cfdc1b33-9824-47d7-9b74-5156342c384d`,
Codex root. Source inspected at `dc0c3fb32`; recheck against current canary before
implementation. No immediate-generation interrupt is implemented by this draft.

- `persona/salience.rs` already computes typed salience; `attention.rs` owns her
  interrupt threshold. Extend these owners rather than adding an amygdala service.
- `airc_persona_conversation.rs::install_stream` owns the independent input pump.
  It signals `directed_pending` after delivery to the bounded inbox. This can wake
  a consumer while the persona is generating; it need not wait for her tool call.
- `cognition/directed_pending.rs` already has generation-tagged acknowledgment
  and a race-aware Notify wait. Reuse that mechanism. A fresh interrupt must be
  distinguished from the event already represented in the current prompt.
- `llm_deliberation_faculty.rs` selects on directed input while waiting for a
  non-directed serving lane, but awaits `generate_stream_checked` without that
  selection after acquiring the lane. Directed work also needs new-input handling;
  being in an already-directed turn must not make her deaf to a later correction.
- `act_observe/settle.rs` admits `perceive_ready` input between steps. Preserve its
  working memory, original task and causal root when restarting deliberation.
  Do not cancel the whole action/observation transaction to interrupt inference.
- `ai/openai_adapter.rs` has a partial-stream cancellation regression verifying
  client permit release and cache bookkeeping. Its comment explicitly excludes
  proof that cancellation stops a production backend. Do not treat that fixture
  as evidence that GPU work has stopped or the serving slot is reusable.

The first implementation must carry the input generation represented by a prompt
through dispatch, observe newer admitted input during inference, suppress stale
tool dispatch, and return to input admission without losing the current task.
Observe-before-compose is necessary: taking a new baseline only after composing
the request could hide a correction that arrived during composition. Resuming
unchanged means continuing the retained task, not promising bit-identical KV or
resumption at an arbitrary decoder instruction. Record deliberate interruption
separately from inference failure and do not consume a failure-retry budget.

External research informs the mechanism and test shape:

- [Tokio select documentation](https://docs.rs/tokio/latest/tokio/macro.select.html):
  losing futures are cancelled; cancellation correctness depends on the operation.
  A select branch cannot interrupt synchronous code blocking its executor thread.
  Therefore race only the inference lifecycle, not a future that may execute tools.
- [InterruptBench](https://arxiv.org/abs/2604.00892) studies requirement addition,
  revision and retraction during ongoing tasks. Use those distinctions in local
  acceptance cases, plus an input that warrants continuing unchanged; the paper
  does not establish this implementation's correctness or performance.

Acceptance must include a delayed real adapter request, a correction arriving
before completion, no execution of the superseded tool call, a subsequent prompt
containing both retained task and correction, and backend/slot release evidence.
Also check arrival during composition and simultaneous completion/interrupt.
Provider cancellation semantics are the remaining prerequisite for production
wiring, not a reason to defer the source investigation or add a parallel runner.

## First implementation and acceptance slice

Start with ordinary coding investigations in the existing room-driven workflow:
multiple plausible defect causes, one useful distinguishing check, then an
updated diagnosis from the actual result. Reuse existing turn capture, review,
and learning paths. Store predictions before outcomes; do not reconstruct them
after success. Train only from appropriately reviewed outcomes, not speculative
claims copied from the hypothesis record.

Compare against the existing workflow on held-out tasks, including ordinary
bugs, misleading logs, correlated reports, and cases with no adversary. Measure
diagnostic correctness, wasted tool calls, latency/token cost, false accusations,
missed adversarial causes, and revision after disconfirming evidence. Score
numeric calibration only where explicit probability forecasts are collected.
Reward useful correction and accurate uncertainty rather than agreement with
an operator or dramatic explanations. Preserve valid minority explanations and
check retention on subsequent tasks before claiming continual-learning benefit.

Implementation decisions still open: the existing owner/type to extend, selective
activation criteria, bounded retention, and the smallest useful evaluation set.
No additional runtime work is authorized by this document alone.
