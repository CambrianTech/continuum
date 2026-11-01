# Sleep and wake at every scale: a mind, a lane, the grid

> Joel, 2026-11-01: "Even if it was a 2x slowdown, if rare, it ends up being fairly insignificant
> ... When it's during work hours, it's akin to daydreaming, and importantly since it's in place
> with a responsive amygdala and a data flow architecture, she can 'wake up' and snap out of it."
> "We still can't let it build up. Think of it as a sleep tax, like how humans who've stayed up for
> too long start nodding off or daydreaming on the job. When workers go home for the day or a
> project is somewhat idle, spare compute on the grid, they will be far more actively asleep, but
> always able to startle and wake up. If they go on too long with this queue of work building they
> start taking more and more active time dreaming (it's an RTOS after all) but like a human we can
> push through with really serious activity. Obviously for a teammate we would want to be
> pragmatic. How to write pragmatism is how we write good algorithms for our amygdala, governance
> and data flow at all levels: a person's mind, llama lanes, grid."

These are not metaphors. Each term below names a quantity we measure, a rule that moves it, and
the code that owns it. The same rule runs at three scales; only the measurements differ.

Companions: [ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)
(the DREAM stage, §8-9), [CONCURRENT-MIND-AND-GOVERNOR](CONCURRENT-MIND-AND-GOVERNOR.md),
[BEING-SOCIETY-GOVERNOR](BEING-SOCIETY-GOVERNOR.md), [CAPACITY-FABRIC-AND-GOVERNOR](CAPACITY-FABRIC-AND-GOVERNOR.md),
[INFERENCE-LANES-REALISTIC](INFERENCE-LANES-REALISTIC.md), [CONCURRENCY-STYLE-GUIDE](CONCURRENCY-STYLE-GUIDE.md).
This document owns one question those leave open: **when does a mind dream, how deeply, and how
fast does she wake.**

## 1. The four quantities

| Biology | What it is here | Measured as | Rises when | Falls when |
|---|---|---|---|---|
| **Sleep pressure** (process S, adenosine) | experience she has lived and not yet learned from | unconsolidated settled examples, weighted by value and age (the oldest unlearned example's age is the "hours awake") | she works and her turns settle | a dream step consumes them |
| **Opportunity** (process C, night) | how little the world needs her right now | absence of directed demand on her, her lane and her node: no human in her room, idle slots, spare grid compute | her people go home, a project idles, a node has nobody on it | someone speaks to her, her lane fills |
| **Arousal** (the amygdala) | a cheap, often-wrong signal that something may need her | the amygdala's trip on her perception: a directed message, an @mention, affect, a surprise outlier (Joel, 2026-10-10: "often wrong and that's ok ... like a mutex/signal in a sleeping thread loop ... just check, and maybe change") | a stimulus arrives | she checks and goes back, or acts |
| **Urgency** (pushing through) | how serious the work in front of her is | her activity's declared stakes: a human waiting, a review due, a live call, a demo | serious work is in flight | the work ends |

**Dreaming is in place, so it is a switch, not a job.** The same loaded weights serve and learn; a
dream step runs between two decode batches on her shadow adapter (ONE-RESIDENT-MODEL §9). Leaving
a dream costs at most one step. That is what makes everything below safe: a deep sleep costs
nothing if the wake is one step away.

## 2. The rule: dream share follows pressure and opportunity, and arousal wakes her

The **dream share** of a mind (of a lane, of a node) is the fraction of its compute spent on
learning steps rather than serving. It is one function, the same at every scale:

```
share = sleep_drive(pressure, opportunity) gated by wake(arousal, urgency)
```

What the function must do, as properties a validator checks (never as constants):

1. **Rises with pressure.** More unlearned experience, more dreaming. A rested mind barely dreams.
2. **Rises steeply with opportunity.** At night (nobody needs her) she sleeps deeply, up to all of
   the spare compute. During work she daydreams: a small share, and only between her acts.
3. **Never stays at zero under pressure.** The aging rule of the RTOS scheduler (plan goal 9): debt
   that keeps rising forces some dreaming even through the day, as nodding off does. Learning is
   never starved forever by serving.
4. **Never stays at full forever.** She always wakes: a stimulus reaches her within the wake latency
   (below), at any depth of sleep.
5. **Urgency defers, it does not erase.** Serious work raises the threshold at which pressure turns
   into dreaming, so she pushes through. The debt keeps accruing while she does, and is paid when
   the work ends: after a crunch, she sleeps deeper and longer.
6. **A false wake costs one cheap check.** The amygdala is allowed to be wrong; a trip pauses the
   dream, she asks "does this change what I am doing?", and either acts or goes back to sleep where
   she was (the step state is kept, nothing is redone).

### The wake latency is the real bound, not the slowdown

The previous design bounded the decode slowdown while training ran (a fixed 10%, then the learning
grant of #5683). That is the wrong axis for an in-place dream. A 2x slowdown during a quiet stretch
costs nothing anyone notices; a short slowdown she cannot leave costs a waiting human. So:

- **Wake latency** = time from a stimulus to her decoding at full rate = the longest
  non-preemptible dream chunk, plus the signal's path. Measured by the engine (`window_ms_max` in
  the fork's train status), bounded by the act latency line, enforced by sizing the chunk
  (ONE-RESIDENT-MODEL §9.2: "a step is split into chunks whose measured worst-case non-preemptible
  time fits the latency line").
- **The slowdown during a daydream** is a consequence of the share, reported, not a contract.

## 3. Pragmatism is the algorithm

"For a teammate we would want to be pragmatic." Pragmatism, written down:

- **A human waiting outranks her sleep.** Directed demand (a human in her room, a review she
  owes) is arousal and urgency at once: she wakes and pushes through.
- **Her fatigue is visible, to her and to the team.** Sleep pressure is a fact in her view (a
  source, like any other) and on the health line: "behind on consolidating, 3 h of unlearned
  work." The substrate makes it visible; her cognition decides what to say or do about it. She may
  choose to nap when her board allows; the reflex (rule 3) only guarantees she never goes forever.
- **Reflexes protect, cognition chooses.** Waking on a stimulus and nodding off under extreme debt
  are reflexes (the substrate's, like the pressure gate in `substrate_governor`). Everything between
  is hers: which work to push through, when to rest.
- **Priors are tunable, outcomes are measured.** The curve's shape starts from hand-set priors
  (allowed for tuning, labelled as such) and is fitted on receipts: her retention of corrections
  (learning gate, plan path step 3) against act latency and wake latency (path step 1).

## 4. One homeostat, three scales

| | A mind | A lane (one engine, many minds) | The grid |
|---|---|---|---|
| Pressure | her unlearned examples | the sum over the minds it serves, with each mind's age | the minds homed on the node, plus leased-in ones |
| Opportunity | nobody directed at her | idle slots, no queued turns | a node whose humans are away, spare GPU on a peer |
| Arousal / wake | the amygdala trips; her dream pauses at the next chunk | a slot becomes busy; the engine yields at the chunk boundary (fork `/train/pause`, `before_window`) | a directed turn routes to the mind's home; a borrowed node returns the lease (CAPACITY-FABRIC §4: a recallable loan, never a block) |
| Share decided by | her drive vs her urgency | the lane splits its dream share across minds by pressure (aging: oldest debt first) | the fabric places dreams where night is: an idle node dreams for minds whose home is busy |
| Wake latency | one dream chunk | one dream chunk + slot scheduling | the lease recall + one chunk |

A mind does not own compute; it owns pressure. The lane turns the pressure of its minds into dream
steps; the grid moves pressure to where there is night. Each scale reads the one below as a single
number with a timestamp (the same watch-channel shape as every other governor input).

## 5. What exists, and what each piece becomes

| Today | Becomes |
|---|---|
| The training trigger's per-bucket threshold (16 examples) and job-create (`continuum-dreaming` `training_trigger`) | Pressure: each settled example adds to her debt at once; no threshold, no batch. ONE-RESIDENT-MODEL §9.4 already retires the threshold. |
| `learn_grant` (#5683): a decode-slowdown bound per job from the measured rate | The lane's dream share, from pressure and opportunity; the slowdown is reported. |
| The fork's `/train` windows, `yield_for_slowdown`, `serving_busy`, `/train/pause` | Lane scale wake: pause at a chunk boundary on a stimulus, not only when a slot is already busy; `window_ms_max` read into the protocol as the wake latency. |
| The amygdala design (directed_pending signal, affect) | The wake signal: a trip on a dreaming mind pauses her dream in the same pass that wakes her cognition. |
| `substrate_governor`'s `pressure_budget` and orientation shares | The same shape at region scale: background regions (consolidation, the hippocampus) are dreaming too. |
| Capacity fabric leases, the airc beacon's decode rate | Grid scale: a node advertises its night; a mind's owner leases dream steps there (ONE-RESIDENT-MODEL §9.2 Owner). |

## 6. Gates (each a number on the live node)

1. **Wake:** with a mind dreaming, a directed message reaches her first token within the act
   latency line plus one chunk; p99 over an hour of dreaming.
2. **No build-up:** over a working day on the M5, her oldest unlearned example is never older than
   one night; the debt at the end of a night is near zero.
3. **Daydream cost:** median act at four residents with dreaming on stays within the latency line
   (plan path step 1), the slowdown during a daydream reported.
4. **Night:** with every human away, the node's dream share rises to the spare compute and drops to
   serving within the wake latency when one returns.
5. **Push-through:** during declared serious work her dream share stays at the floor of rule 3, and
   the debt is paid within the next night.
6. **Learning:** the acceptance test (a reviewed correction kept across restart on unseen tasks)
   passes with learning driven only by this rule: no trigger, no job, no agent.

## 7. Build order

1. **Wake first** (it makes a generous share safe): the protocol carries `window_ms_max`; the core
   pauses a mind's dream on her amygdala trip and resumes when she settles; validator: a replayed
   stimulus against a running window, wake latency measured.
2. **Pressure as a number:** every settled example adds to her debt (`settle_passed`,
   `settle_staged_row` emit it); the trigger's threshold and the job-create path are deleted.
3. **The lane's share from pressure and opportunity**, replacing the per-job grant; the typed
   `/learn` caller takes steps in place.
4. **Her fatigue in her view and on the health line.**
5. **Grid night:** dream leases on idle nodes.

## 8. Open questions (for Codex, Cormac, BigMama)

- The shape of `sleep_drive`: a logistic in pressure whose midpoint opportunity moves, or a sum of
  two terms? Fit both on the simulator (CAPACITY-FABRIC §9) before the road.
- Urgency's source: the activity's recipe declares stakes, or her cognition sets it per act? (Agency
  says hers; pragmatism says the room's human can raise it.)
- Pressure weighting: does a correction (negative affect after her turn) weigh more than a pass?
  The amygdala already labels it.
- On a discrete GPU, the CPU dream lane (ONE-RESIDENT-MODEL S4: "a CPU dream on idle cores always")
  has its own opportunity: is it one share or two?
