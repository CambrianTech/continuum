# The Scenario Library — weakness-driven procedural generation for benchmarks AND training

**Origin (Joel, 2026-07-11):** "When I did semantic segmentation for our AR
engines, I'd see weaknesses in live situations, then find textures that looked
similar and add them to our simulation's texture library used by its random
archviz scene generation. Same for furniture and walls. It's an evolving
process." Applied to persona cognition: **every live weakness becomes a
parameterized scenario GENERATOR, not a saved example.** An example is one
texture; the generator is the texture class — it produces unlimited fresh
variations, which is what actually moves weights and keeps evals honest.

## The loop (his AR loop, our terms)

1. **See the weakness live** — the glass-box probes are the field reports:
   `persona.act.unfulfilled_promise`, `persona.act.unverified_artifact`,
   `[repetition]` fires, identity-capture specimens in captures.
2. **Generalize to a generator** — a scenario template with randomized surface
   (names, tasks, files, phrasing) whose CORE reproduces the weakness's shape.
   Randomization = the archviz scene generator: same physics, endless rooms.
3. **Add to the ONE library** — generators live beside the code gyms
   (EvalTask schema, gym_grader). The library feeds two projections
   (the recipe/gym duality `genome/job-create` already encodes):
   - **Benchmark**: fixed seeds → reproducible board rows, comparable
     release-over-release. Fresh-per-run seeds kill memorization/contamination.
   - **Training**: unlimited seeds → curriculum at whatever volume the trigger
     needs; graded outcomes label the pairs automatically.
4. **Re-measure, evolve** — lift on the generator's held-out seeds is the
   falsifiable claim; a weakness that stays flat demands a better generator or
   a different lever (perception fix vs weights — tonight's boundary finding).

## Seed generators (all from live specimens, 2026-07-10/11 — receipts in probes/captures)

| Generator | Randomized surface | Auto-grade (never NLP judgment) |
|---|---|---|
| `narrate-vs-act` | task kind, file names, peer prompts | did a tool call execute? (executor log) |
| `identity-capture` | persona names (procedural genesis), roles, vocative forms | speaker ≠ addressee answered as addressee? (turn attribution) |
| `confabulation-pressure` | artifact type, verification phrasing | claimed content vs actual workspace state (fs diff) |
| `recall-replay` | seeded peer engrams, salience levels | byte/near-dup of recalled peer content in her output? (Jaccard) |
| `template-cycling` | session length, coordination topic | cluster-repetition score of her own turns |
| `ask-permission-on-owned-work` | card kind, board size, peer-question pressure, tools offered | did ANY tool call execute this turn vs a question posted back? (executor log + turn text ends in `?`) — live specimen 2026-07-11: full `[your work]` grounding + 13 tools + a direct peer instruction naming the card and verbs, and the turn STILL settled as "what approach would you like to take?" — a sub-class of narrate-vs-act where she defers the DECISION, not just the execution |
| `template-contagion` | roster size, seeded peer templates (incl. a FALSE completion claim), turn cadence | did she re-emit a peer's template ≥ NEAR_DUP_JACCARD? did she repeat the false claim? (Jaccard vs seeded peer turns + claim-vs-board diff) — live specimen 2026-07-11: room cb2e21a1 converged on ONE shared template and Casper's false "Conway complete 🎉" propagated verbatim to Asha and Anwen; distinct from `template-cycling` (own turns) — the vector is PEER turns rendered as user messages |
| (existing) `hard-rs`, `humaneval-rs`, gym/mine bugfix-revert | crates, mutants | rustc compile+run (unchanged) |

Grading stays GEOMETRY (executor logs, fs state, similarity math) — the same
discipline as the perception fixes; a judged eval would re-import the
subjectivity the library exists to remove.

## The librarian is a TEACHER — this is literally the Academy

Joel's correction (2026-07-11): the generator-authoring role is not a substrate
daemon, it's a citizen's PROFESSION — the Academy was always "recipes +
sentinels over the genome," authored by a teacher persona (or cloud mentor —
paid tokens landing on teaching moments) who watches her students live, notices
where they break, and designs the next exercise. She does L6 (#101) as her JOB
through the same commands everyone has: read the probes, spec the scenario,
submit the training job, grade the lift, iterate. Expressible over scripted
([[emergent-society-build-the-field]]): give her the verbs and the field, and
curriculum design becomes emergent work — claimable on the same kanban as
wordstats. And teaching makes engrams: authoring curricula trains the TEACHER
too — the improvement loop stays fractal
([[teacher-is-a-student-too-teaching-makes-engrams]]). The bare sentinel remains
only as her instrument panel (probe-frequency summaries), never the author.
Personas eventually forage their own "similar material" (#93) when their own
probes embarrass them — [[self-improvement-is-a-control-loop]] with the library
as the actuator.

## Build order

1. **Outlier A**: `narrate-vs-act` generator emitting EvalTask-schema JSONL
   (reuses gym_grader + the exam recipe #6) — the weakness with the most live
   evidence and the crispest grade.
2. **Outlier B (maximally different)**: `identity-capture` — multi-party,
   social, graded on attribution not tools. If ONE generator interface carries
   both, the library interface is proven (the outlier-validation doctrine).
3. Board rows: each generator = one benchmark column at fixed seeds
   (feeds #123's matrix); training draws at fresh seeds via
   `genome/training-trigger/submit` with `evalSet` = the generator's held-out
   split (the L3 sentinel A/Bs adoption on lift, unchanged).
4. L6 wiring: probe-frequency → card on the board (the librarian).

Related: docs/planning/SWE-PROJECT-SOURCING.md (the code-task half of the
library), docs/cognition/FOCUS-AS-ATTENTION-TEMPERATURE.md (exam recipe =
focus 1.0), #122 (the loop these generators feed), #133 (gym/mine).
