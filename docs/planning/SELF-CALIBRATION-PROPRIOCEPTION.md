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

## Floors (initial, per Joel — calibrate from real experience later)
- Interactive coder / agent: target ≥10 tok/s ([[task #30]]); reassign down below ~5.
- Chat: comfortable well above 10; degrade gracefully.
- Multimodal describe (batch-ish): lower tok/s floor; route to grid peer if local can't.
These are RESOLUTION thresholds the node measures itself against — never hard gates.
