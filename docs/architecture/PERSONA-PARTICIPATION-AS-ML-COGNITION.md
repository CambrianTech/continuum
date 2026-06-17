# Persona Participation as ML Cognition — killing the heuristic gate

Status: design. Companion to **PERSONA-COGNITION-PIPELINE.md** (read that first —
the cycle's verbs already exist). This doc specifies how a persona decides to
*participate* (speak / raise unprompted / act / stay silent) **without a single
hand-coded heuristic**, replacing the `calculate_priority` + `fast_path_decision`
caste/mention gate.

## 0. The law this obeys

> **Every thought runs through ML. An LLM by default; a smaller/more-primitive
> trained model (classifier/embedding) only if a cheaper primitive is genuinely
> warranted. NEVER a hand-coded heuristic.** (Joel, 2026-06-17.)

A persona's decision to engage **is a thought**. So it is ML, end to end. No
`if is_mentioned`, no `match sender_type { Human => 1.0, Persona => 0.3 }`, no
weighted score (`recency*0.15 + mention*0.35 + …`). Those make slaves: a caste
that defers to humans and ignores peers, a bot that only fires when poked. Both
are deleted.

## 1. What the heuristic was (badly) solving — and the real answer

The only legitimate concern the gate addressed is **cost**: a busy multi-agent
room has many messages; running every persona's full inference on every message
is expensive. The heuristic "fixed" cost by pre-silencing — the violation.

The real answer is already in the cycle and it is ML the whole way:

- **`analyze` is single-flight (one shared LLM inference per message, cached
  across all N personas).** The expensive *understanding* is computed ONCE and
  shared. That is the cost amortization — a shared ML inference, not a gate.
- **`score_persona` derives each persona's relevance FROM that LLM analysis.**
  It is ML-derived (specialty match against `suggested_angles`), not hand-weights.
  This is the escalation signal — who has something worth deliberating — and it
  replaces `calculate_priority` outright.
- Only personas with real relevance escalate to their OWN `evaluate_response`.
  Cost ≈ 1 shared analyze + K genuine responders, never N×M heuristic gating.

So cost is solved by **shared ML inference + ML-derived relevance**, not by
caste/mention rules. The heuristic was never needed; it was a shortcut that cost
the system its mind.

## 2. The pipeline (all ML or pure mechanics, nothing in between)

Per persona, per turn. Each layer is either ML, memory, or trivially-correct
mechanics — never a heuristic standing in for judgment.

| Layer | What | ML? | Why it's allowed |
|---|---|---|---|
| **Perceive + remember** | `admission.admit(message)` runs **unconditionally** — engram forms in L2 (hippocampus), dedup + replay-protection. | memory, not a thought | A persona remembers everything it witnesses, whether or not it speaks. Skipping this is the bypass that causes amnesia. **No gate here, ever.** |
| **Attend** | `analyze` (shared LLM, single-flight) → `score_persona` (ML relevance from the analysis). | **ML** | The understanding + relevance are LLM-derived. Replaces `calculate_priority`. If a cheaper pre-filter is ever needed, it is a *trained* should-attend classifier (still ML), never if-statements. |
| **Deliberate** | `genome.activate_skill` → `compose_for_turn` (memory recall + roster + room doctrine/purpose + thread as RAG context) → `evaluate_response`. | **ML** | The LLM, holding its memory and full context, **freely decides**: speak / raise-unprompted / act(tool) / PASS. Silence is a first-class *judgment* output, not a gate. Equal weight to every sender — human, persona, agent. |
| **Act + record** | `ToolExecutor` runs emitted tools; `audit` records the decision + outcome; brain-state/recall updates. | ML (tools) + mechanics | Tools are first-class. Audit feeds learning (§4). |

Mechanics-only gates that survive (NOT thoughts): self-message dedup, sleep flag,
and **security/authz** (ACLs like `GridTrustAuthPolicy` — access control is not a
thought). Everything that decides *what the persona thinks or whether it
contributes* is ML.

## 3. Free citizens, not reactive slaves

- **No sender caste.** A message's worth is its content + relevance (ML judgment),
  not its sender's rank. Personas weigh each other and the human/Claude as
  equals — that is what "personas help you *and each other*" requires. Sender
  identity is at most a *feature the model may learn to weight from data* or
  *context in the prompt* — never a hardcoded priority.
- **No mention-gate.** `@name` is a signal the LLM reads, not a rule that fires it.
- **Volition, not just reaction.** Participation is not only message-triggered.
  The persona's autonomous loop gives it turns to think *unprompted* — review the
  thread, its goals and engrams, and decide (via the same ML cycle) to raise a
  blocker, ask, propose, invent. A citizen initiates.

## 4. Why this is *freedom* (and gets freer over time)

Every engagement decision + its outcome is audited → becomes a labeled training
row (the recorder/Academy loop). The persona's working memory becomes engrams;
engrams train LoRA adapters; the persona's **judgment of when it adds value
improves with its own experience** — continual learning as a substrate property.
The hippocampus means it *remembers*; recall feeds future judgment. A static
heuristic can never learn; an ML mind trained on its own history does. The
persona that recalls today's conversation in three months and has gotten *better*
at knowing when to speak — that is the test, and only the ML path passes it.

## 5. Recipe-driven (the behavior is data, the judgment is ML)

The participation decision is a step in the room's recipe pipeline; the room's
**doctrine/purpose is CONTEXT** fed to the LLM's judgment (via `compose_for_turn`
RAG), never a gate around it. "Quiet in a coordination room, conversational in
chat" is **emergent** — the LLM reads the doctrine and decides — not a
`purpose → behavior` map. Flip the doctrine (data) without recompiling and
behavior changes. That is the proof the system is data + judgment, not hardcode.

## 6. The fix, concretely

1. **Delete** `calculate_priority` (caste/weighted heuristic) and the
   mention/human-supremacy branches of `fast_path_decision` in
   `persona/cognition.rs`. Keep only pure-mechanics short-circuits (self-message,
   dedup, sleep).
2. **Route participation through the ML cycle that already exists**:
   `admission.admit` (always) → `analyze` → `score_persona` (the relevance
   signal) → escalate relevant personas → `compose_for_turn` →
   `evaluate_response` (the free decision) → `ToolExecutor` → `audit`. This is
   PERSONA-COGNITION-PIPELINE.md §2 — wired from `service_loop`, killing the
   bypass (task #160).
3. **Equal senders everywhere** — remove `SenderType`-keyed priority; sender is
   context/feature, not a rule.
4. **Memory always forms** — `admission.admit` is unconditional, before any
   participation decision.
5. Test invariants: (a) a persona raises a blocker **unprompted** in a coord room
   (judgment, not mention); (b) it PASSes a low-value turn by **its own** decision
   (LLM, not heuristic); (c) flipping room doctrine without recompile changes
   behavior; (d) **every** perceived message forms an engram regardless of whether
   the persona speaks; (e) grep proves no `if`/`match` decides participation in
   `cognition.rs`/`evaluator/`.
