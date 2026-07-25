# Emergent team organization — trained dynamics, never hardcoded roles

Joel (2026-07-23): *"We need to train for team dynamics, NOT hard coded roles.
We expect emergent organization. I saw it before in prototypes. It just happens
if they're freely communicating with one another and you haven't hamstrung
this."*

This doc pins what the substrate already gets right, what was strangling
emergence in practice, and how team competence is grown — so no future session
reflex-codes a role system into cognition.

## The doctrine

1. **Roles are resource hints, never behavior.** `RoleId` (Helper/Coder/…)
   exists for the SPAWNER — model-tier selection, spawn priority. Verified
   2026-07-23: it never enters prompt assembly; every `.role` on the prompt
   path is a chat-message role. Keep it that way. A "manager" is something a
   persona BECOMES in a room, not something we stamp on her.
2. **Identity is self-authored.** `persona/identity/set` gives each persona an
   open profile (bio/goals/desires/interests/…), self-editable, id-immutable
   ([[persona-identity-is-fully-self-editable-except-the-id]]). The operator
   INVITES authorship; the operator does not write personalities. Hand-authored
   personas are hardcoded roles by another name.
3. **Emergence needs three conditions, all substrate-level:**
   - **A free channel** — messages heard in the room they arrived in, replies
     landing where addressed, memory recording WHO spoke. (The 2026-07-23
     purification arc: nil-room turn fix, kill-verify serving gate, roster-named
     engrams, reply-routing. 22,064 turn starts / ONE spoke in a day was a
     strangled channel, not a social failure.)
   - **Heterogeneity** — differentiated selves. Glass-boxed 2026-07-23: all
     four residents had `profile: {}` — EMPTY. Interchangeable minds cannot
     divide labor. Differentiation compounds from: self-authored identity,
     divergent genomes (per-persona LoRA stacks), divergent memory (lived
     history), and — when hardware allows — different base drivers.
   - **Shared stakes** — a common work surface (the airc kanban/cards) everyone
     can see (requires the room-context fix) and claim from freely.
4. **Team dynamics are TRAINED from lived traces, not prompted.** The
   coordination gene's corpus is the room's own multi-party history — real
   claims, yields, hand-offs, reviews, acks (`dataset/from-turns` — the
   [[coordination-learning-flywheel]]). Never a synthetic "you are the manager"
   script. Better dynamics → richer traces → better corpus: the flywheel.

## The emergence meter (objective, from existing probes)

A forensic pass on any team session reads, per day:
- `persona.turn.spoke` / `persona.turn.start` — is the channel alive at all?
- peer→peer reply chains (spoke in response to a peer's spoke, not operator)
- card claims + completions per persona (initiative)
- identity self-edits (are they differentiating?)
- hand-off events (one persona's output consumed by another's next act)

The prototypes' "it just happens" becomes falsifiable: these counters move, or
the substrate is hamstringing again and the fix is a bug hunt, not a prompt.

## Sequence (2026-07-23 state)

1. Deploy the four channel fixes (waiting on teach-he-r4's lane).
2. Invite self-authorship: one directed message per persona — write your own
   bio/interests/values via `persona/identity/set`.
3. Commission real work (website tier card) to the ROOM — one broadcast, no
   assignments — and read the emergence meter.
4. When traces accumulate: mine the coordination corpus, train, sentinel-gate,
   and measure team throughput (cards completed/day), not individual scores.

## Speciation — how differentiation compounds (2026-07-23, Joel: "collaborative
## development and speciation… would happen naturally if they are working together")

Speciation is divergent experience compounding through the personal learning
loop. The substrate's job is to keep the loop PERSONAL and the information
FLOWING — never to assign niches.

1. **Role-asymmetric experience (live now).** A team run gives the solver and
   the reviewer DIFFERENT lessons from the same task (`--learn` on every
   benchmark dispatch as of today — work IS training). A persona who reviews
   accrues review engrams; her dreams distill review beliefs; her next gene
   trains on her own review trajectories. Same task, two diverging minds.
2. **Genes train on a persona's OWN trajectories only (doctrine).** One shared
   gene averaged over everyone is homogenization — the anti-speciation force.
   Cross-pollination happens through EXPLICIT channels: engram transfer
   (telepathy, permissioned), teaching rounds (the teacher forms engrams too),
   and published genome layers another persona may TRY and keep only on
   measured lift (sentinel-gated adoption = horizontal gene transfer with a
   fitness test).
3. **Competence is visible, never prescriptive (small build, next).** Mine the
   run ledgers into a per-(persona, task-kind) track record; surface it into
   each persona's OWN identity RAG ("my last four reviews caught the defect")
   and the room roster (teammates see who lands what). Claims stay free — the
   record is information, not routing. Preferential attachment does the rest:
   whoever reviews best gets asked to review, reviews more, diverges further.
   The selection pressure is the benchmark ledger itself.
4. **The efficiency law: remove operators, not steps.** The flywheel's speed is
   set by its manual seams ([[genome-loop-manual-seams-are-the-autonomic-build-list]]).
   Autonomic order: dream triggers training when a persona's corpus crosses
   threshold (in her own downtime window, governor-refused if serving needs the
   memory) → sentinel measures → adopt/reject → the next day's work exercises
   the new reflex → new traces. Nobody schedules it. The operator's only
   remaining verbs are commissioning work and merging to main.

## Anti-patterns (refuse these in review)

- A `TeamRole` / `manager` enum consulted by cognition or prompt assembly.
- Operator-authored persona bios ("make Asha the designer").
- Turn-taking schedulers that decide WHO should answer (the evaluator decides
  engagement per-mind; scheduling hygiene like loop-dedup is fine — role-aware
  routing is not).
- Benchmarks that score a persona out of the society she works in
  ([[eval-measures-the-true-full-being-not-a-stripped-copy]]).
