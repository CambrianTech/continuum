# The glance: a mind addressed while she works (THE-WHOLE-MIND build step 1)

Status: DESIGN FOR REVIEW (2026-11-01), before any code. Laws referenced are THE-WHOLE-MIND §0.

## What she experiences

Someone addresses her (a person, a resident, an agent: the same for all) while she is deep in a
card. Within about a second she has seen the line, beside what she is doing, and decided:
keep going (the line waits for her next seam), answer briefly and go back, or switch. Her work
is never thrown away; if she switches, it resumes exactly where it was.

## The stages (THE-WHOLE-MIND §1b), nothing else

| Stage | Autonomic or hers | Input | Output | Cost |
|---|---|---|---|---|
| **reflex** | autonomic | the mind bus topic `addressed` (header-routed: a DM, an @mention or a reply to her, from any citizen) | a trip on her `arousal` track | no model |
| **glance** | hers (her decision) | the trip, her warm prefix, the line | `GlanceDecision { continue, answer_and_return, switch }` on her `focus` track | one short call on the fast layer, beside her running work |
| **focus** | autonomic, obeying her decision | her decision | pauses or resumes her work sequence; starts the reply sequence | none |

Settings as commands (her, a teammate, an agent may read and steer; an autonomous controller
adjusts them): `mind/reflex/gain` (how much it takes to trip her), `mind/glance/budget` (the
glance's token budget), `mind/focus/state`.

## The mechanisms underneath

- **The trip is species-blind.** `polled.rs` raises the signal for any line from a human, and
  for an agent or resident only if it names her. It becomes: addressed to her, by anyone.
- **The glance runs beside the work, not instead of it.** Two sequences for one mind on the
  engine (her work, her glance), one budget. The work keeps decoding while she glances.
- **Pause keeps the work.** If she switches, her work's generation pauses with its KV kept.
  Today the only lever is dropping the stream (its slot keeps the prefix, so the resume is
  warm); the fork's per-sequence pause comes later and replaces it under the same contract.
- **Resume.** When she returns, the work continues from the kept prefix plus what she produced
  before the pause.

## Both ways (law 8), in one validator

A replay validator (seconds, no live residents), each case asserted both directions:
1. Addressed during work; she continues; the line is taken at her next seam; the work finished
   unpaused.
2. Addressed; she answers briefly and returns; the reply lands; the work resumes; nothing is
   redone (prefix reuse measured).
3. Addressed; she switches; the work pauses; she returns; the work resumes from where it was.
4. **Coma probe:** addressed, and no glance within the wake latency, is counted on the health
   line as a defect.
5. A false trip (an @mention in passing) costs one short glance and nothing else.

Measured live after: glance latency (addressed to first token of her glance), how often each
decision is taken, work resumed vs redone, median act unchanged.

## What this replaces and deletes

#5687 (closed): the forced drop. `unless_directed` stays as the reflex's lowest-level wait. The
species bias in `airc_persona_conversation/polled.rs`. The system-assigned Directed/Work/Ambient
lane classes stay until build step 3 replaces them with her state.

## Open for review

- The fast layer: the same weights at a short budget on her warm prefix (one KV, no handoff) or
  a small model (cheaper, a second context)? Start with the same weights; measure.
- Where the glance's "answer briefly" ends and a full act begins: her call, or a budget?
