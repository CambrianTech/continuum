# Society HUD — the WHO/WHAT/WHICH shell, terminal-cyberpunk skin

Live mockup (2026-07-23, real session data): https://claude.ai/code/artifact/1ef00901-202b-4900-b1c5-ec3ad142a1c5

The spectator view of the organism ([[academy-learning-is-the-show-spectator-entertainment-thesis]]):
one screen where a user watches the society hear, claim, work, review, dream,
and change its mind. Every element renders EXISTING substrate surfaces — no new
cognition, only faces for what already emits.

## The three panels (Joel's shell semantics, desktop/iPad)

| panel | scope | positron widget | data source (already emitting) |
|---|---|---|---|
| **WHO** (left, global) | persistent across activities | `persona-tile` list | persona state (energy/attention/mood), genome stack + scales (`serving/status` adapters), track record (`~/.continuum/team/track-record.json` — competence miner) |
| **WHAT** (center) | the current concern | `room-feed` + `work-board` + `emergence-meter` strip | airc room transcript, `airc work board`, emergence meter counters (spoke/start, voices, peer-chains, claims, self-edits) |
| **WHICH** (right, many) | selected sub-view | `mind-inspector` (tabs: supersession, lane-governor, dream, belief-graph, engrams, recall trace) | dream probes (`hippocampus.decay`, review passes), supersession verdicts (SUPERSEDES lines), governor lease grants/refusals, `serving.plan` |

## The moments that make it a show

- **Supersession stream** — old belief struck-through → new belief, with the why
  ("flask-4045 post-mortem · gene training tonight"). Watching her change her
  mind is THE differentiator ([[beliefs-are-defeasible-agm-not-axioms]]).
- **Governor refusals rendered as wins** — `train-lease REFUSED 22G>18G → queued
  for downtime` shows the machine protecting itself (this exact event happened
  today; it used to be a kernel panic).
- **Emergence meter strip** — the society's self-organization as five live
  counters. Zero-state is honest ("board quiet — invite work").
- **Dream pacing modes** — eco / fast / ludicrous as a visible, clickable
  governor profile (the business-hours knob).
- **Track rings on tiles** — speciation visible: Anwen's `swe 1/1 · web 1/1`
  vs Benchy's "unproven — invite work".

## Visual language

Committed single-theme: CRT phosphor terminal (near-black green ground,
phosphor-green primary, amber = attention/live, cyan = identity, red = refusal/
supersession-old). Scanline + vignette overlays, `prefers-reduced-motion`
honored. Monospace throughout (system mono stack). This is the README's
90s-terminal/cyberpunk direction; positron themes can later offer the same
widgets in daylight skins — the widget contract is theme-free, the skin is not.

## Build order (positron)

1. `emergence-meter` strip — smallest, reads one JSON, immediately useful in ANY room view.
2. `persona-tile` with track rings + genome chips (the live-game-HUD memory).
3. `supersession-stream` — tail the dream probes; the flagship "she's alive" moment.
4. `work-board` + claim highlighting in the room feed.
5. `mind-inspector` tab shell (lane governor first — it's one probe class).
