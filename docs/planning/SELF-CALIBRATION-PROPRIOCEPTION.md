# Self-calibration — the system finds its own bearings, per activity, continually

**Status:** principle locked 2026-07-29 (Joel): "The system will find the most natural
fit... anything sub 5 tok/s is probably where it'd reassign to lower intelligence, but
it needs to know how it can do with a devoted coder for example, or the multimodal
kinds... the system has to figure out its own bearings, regularly, and know how to
maximize experiences (activities) so it knows how to scale up and down."

This is the layer ABOVE serving. GPU expert paging + measured tok/s
([[K3-GPU-EXPERT-PAGING]]) is the SENSORY INPUT; this layer USES it to decide what the
node should BE, per activity, and to scale intelligence up/down as conditions change.

## The loop (proprioception, not a boot classification)
1. **Probe self, per activity-type, continually.** Regularly run representative probes
   — an interactive coding turn, a multimodal describe, a chat turn, an agentic tool
   loop — at candidate intelligence tiers, and MEASURE sustained tok/s + first-token
   latency + hot_set_hit_rate + quality on the CURRENT hardware / load / grid state.
   This is the benchmark-as-self-assessment ([[benchmark-learning-flywheel]]), run as a
   background bearing-check, not a one-time catalog resolution.
2. **Capability map.** `(activity_type × intelligence_tier) → {sustained_tok_s,
   first_token_ms, quality, needs_grid?}`, continually refreshed. This is what the node
   KNOWS about itself: "for a devoted coder I sustain 14 tok/s at tier-A locally; for
   multimodal I need a grid peer; chat runs tier-A at 30 tok/s."
3. **Assignment = highest intelligence that clears the activity's experience FLOOR.**
   Each activity has its own floor (an interactive coder needs responsiveness — a tok/s
   AND first-token floor; a batch multimodal describe tolerates slower). Pick the most
   capable model that clears it. **Sub ~5 tok/s ⇒ reassign DOWN** to a smaller/faster
   model for that activity — a lower-intelligence experience that stays responsive beats
   a frontier one that stalls (the QualityModel already encodes this: a stall zeroes
   mean_experience via the critical-faculty gate, [[continuum-substrate-already-built]]
   capacity/consumer.rs).
4. **Scale up/down on re-bearings.** Peer joins → capacity up → scale intelligence UP
   (or take a harder activity). Load spikes / thermal / peer drops → scale DOWN to hold
   experiences above their floors. Same code, different bearing — the SubstrateGovernor
   DVFS idea applied to model intelligence, not just clocks.

## Objective: maximize EXPERIENCES, not tok/s
The thing being maximized is the set + quality of ACTIVITIES the node serves well
(`mean_experience`, the RANSAC score in capacity/mod.rs), never raw throughput. A node
that runs one frontier model at 3 tok/s serves fewer good experiences than one that
runs a tier-down coder at 12 tok/s + multimodal via a peer. The self-calibration picks
the intelligence mix that maximizes served experiences under the live resource vector.

## What this realizes vs redesigns (wire, don't rebuild)
- **Realizes:** `QualityModel` / `mean_experience` (the experience reward), the resource
  negotiation (`grant_all` over [[resource_vector]]), `SystemProfile`/catalog (what
  fits). Those exist.
- **The new layer to build:** the CONTINUAL per-activity self-benchmark loop → the
  capability map → the intelligence-tier assignment with per-activity experience floors.
  It is `catalog = f(system × storage × grid)` made (a) continual and (b) keyed on
  MEASURED sustained tok/s per activity, with the ~5 tok/s reassign-down floor.
- **Feeds from:** the paging tok/s + hit-rate meter ([[K3-GPU-EXPERT-PAGING]]) and the
  benchmark flywheel. Measured, never guessed — a self-assessment on stale/guessed
  numbers reassigns wrong.

## The experience score is criticality-GATED, activity/goal-contextual, and temporal (Joel 2026-07-29)

An activity's experience score is NOT an average of sub-scores — it is gated by its
CRITICAL components, shaped by context, and evaluated over a window:

- **Sub-scores + criticality gate.** Each activity has sub-scores (latency, fps, TTS,
  STT, response quality, ...). They combine NON-linearly: a critical component degraded
  collapses the WHOLE score even if everything else is green. Losing TTS or STT in a
  live video chat totally compromises it — the experience score goes low regardless of
  fps. This extends the QualityModel's critical-faculty gate ([[continuum-substrate-already-built]]
  capacity/consumer.rs) from "a crash zeroes experience" to "any CRITICAL-component
  degradation zeroes experience." The map must know which sub-components are load-bearing
  per activity.
- **Activity + goal context.** The score's shape depends on the activity AND the goal:
  video chat with 14 personas vs 3 vs 1 has a different resource profile and different
  critical set; the goal weights the sub-scores. Same node, different bearing per context.
- **Degrade by criticality, not uniformly.** When latency/fps lags, cut the LEAST-critical
  sub-component first (a background persona's avatar fps, a non-speaker's video) to PROTECT
  the critical ones (never drop the active speaker's TTS/STT). Shed to preserve the
  experience, targeted — not a uniform throttle that clips everything including the
  load-bearing parts.

## Temporal concentration — optimize experience over a WINDOW, not each instant

The governor must maximize experience over a TIME WINDOW, not instantaneous fairness. It
is correct to briefly PAGE OUT other personas so a hard task gets the smart MoE (e.g. K3)
for a few minutes — the combined windowed experience is HIGHER because the hard problem
gets solved, even though the instantaneous "everyone served equally" metric dipped. A
governor "too worried about satisfying all personas" every instant never lets anyone do
deep work — the exact failure Joel flagged: it refused to even temporarily admit K3 for a
hard coding problem. Add a bounded, reversible CONCENTRATION term to the negotiation
([[resource_vector]] grant_all): a high-value hard task may temporarily concentrate
resources (page out low-priority lanes), on a deadline, then restore. Reversible +
time-boxed = [[restarts-are-commonplace]] applied to attention.

## Difficulty/failure-driven ESCALATION — scale UP on hard, not only DOWN on scarce

Intelligence scales UP on detected DIFFICULTY, the dual of the ~5-tok/s reassign-DOWN on
scarcity. Detect thrashing / repeated failure / low quality — a coder that can't solve the
hard problem, an agent looping on a task (**like Claude thrashing on the hf download this
very session** — repeating a failing move, not escalating) — and ESCALATE to a smarter
model for that hard stretch (19B→K3; Opus→Fable), then DE-escalate when it's easy again.
The failure/thrash IS the signal ([[benchmark-learning-flywheel]]): graded failure →
escalate. This makes intelligence assignment two-sided: reassign DOWN when the tier can't
sustain the experience (scarcity), reassign UP when the tier can't SOLVE the task
(difficulty). Both detected from measured experience, both temporary, both reversible.

## Multi-user + contention: the human's foreground work is the TOP-priority activity (Joel 2026-07-29)

Eventual goal: run **system-wide (all users)**, a background service serving whoever's on
the box — deferred for now (complexity/safety concerns), but a good eventual test and how
Joel sets up his machines (e.g. his wife's account runs Steam games). The load-bearing
requirement:

- **Recognize the user + what they're doing.** Detect a foreground GPU-heavy app (a game —
  via Steam running / foreground process / a GPU-util spike from a process we don't own) as
  a FIRST-CLASS contention signal, not just a number.
- **The human's foreground GPU work is the HIGHEST-priority "activity"; continuum defers to
  it, always.** "Not be a problem during outside GPU usage, or at least not interfere,
  deprioritize." When the user games, continuum YIELDS: sheds VRAM, deprioritizes/pauses its
  lanes, pages experts out, scales intelligence down, or moves work to the grid — so the game
  gets the GPU. The human's experience is a load-bearing sub-score; **degrade continuum,
  never the human's game.**
- **This is what `capacity/mod.rs` was built for.** `gpu_free_bytes_live` = "free after
  external (unowned) load — a game/browser"; the FitPolicy derives grants from live free, so
  a game opening (shrink) and closing (grow) already fall out. Joel's scenario adds the two
  concrete pieces on top: the SENSOR (foreground GPU app / Steam detection) as an explicit
  high-priority unowned-pressure source, and the POLICY (aggressive yield to foreground user
  work). Makes continuum a good citizen on a shared/gaming machine — the precondition for the
  all-users service. Ties to [[resource_vector]] (measured live free, external subtracted)
  and the always-on background-service end state.

## Floors (initial, per Joel — calibrate from real experience later)
- Interactive coder / agent: target ≥10 tok/s ([[task #30]]); reassign down below ~5.
- Chat: comfortable well above 10; degrade gracefully.
- Multimodal describe (batch-ish): lower tok/s floor; route to grid peer if local can't.
These are RESOLUTION thresholds the node measures itself against — never hard gates.
