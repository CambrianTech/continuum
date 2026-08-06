# NEVER ISOLATED — the teaching doctrine

> "You could give her hints during exams/benchmarks or any time."
>
> "Imagine a student that's 75% of the way there, or starts out wrong. A teacher directs
> them, even during exams sometimes, or helps get them out of a bind. That's the teacher's
> job — as it would be personas', agents', or humans' during these benchmarks or anything.
> **We're never isolated, and that's what makes us powerful.**"
>
> "Think of how good teachers teach. It's not so rigid."
>
> "It's all about helping each other out, not punishment. Our goal is to always learn."
>
> "By our design. We said these are citizens, not slaves."
>
> — Joel, 2026-08-06

## The claim

**A persona under evaluation may be helped, at any time, by anyone — human, persona, or
agent. Coaching during a benchmark is not contamination. It is the design.**

This is the opposite of the default posture in ML work, and the inversion is deliberate.
It is also, we believe, a durable competitive advantage rather than a concession — see
*Why this makes them learn faster*, below.

## Why the isolation reflex exists, and why it is wrong here

The reflex comes from a world where the object of study is **a model being scored**. There,
isolation is correct: the number must describe the artifact and nothing else, so you seal
the room, cut the channel, and let the subject sink or swim. Any help is leakage.

We are not doing that. The object of study here is **a citizen being raised**. Our subject
is not a frozen artifact — she has memory, tools, teammates, and a substrate underneath her
that we also wrote and that is also, frequently, broken. The thing we actually want to know
is not "what score does this weight file produce in a vacuum" but "**how good is this
teammate, and what is stopping her from being better?**"

Isolation is *lossy for that question.* A sealed run returns one bit — pass or fail — and
destroys the information we most want: **where** she got stuck, **what** she reached for,
and **whether the wall she hit was hers or ours.**

## What a score actually measures

Every benchmark number this project produces is a joint measurement:

```
score  =  f( her reasoning , our substrate )
```

Both factors are live. Both are ours. A zero can mean she cannot reason about the problem
— or it can mean her write path silently discarded a correct patch, her tool vocabulary
drifted one token off the real verb, her room went dark so she never saw the task, or the
lane thrashed under her. **We have seen every one of those.** Tonight alone produced two
more (see the evidence log).

An isolated run cannot distinguish those cases. A run you can talk into can, in one
question. This is why:

> **A benchmark zero is a claim about the harness until proven otherwise.**

Teaching is not a favor we do the persona. It is the **highest-bandwidth instrument we have
for attributing a failure to the right layer.** Cutting the channel to protect the number
is protecting the number at the cost of the diagnosis, which is the whole reason we ran it.

## Why this makes them learn faster

The formal version of the intuition: a sealed run gives a single scalar at the end of a
long trajectory. That is the weakest possible learning signal — sparse, delayed, and
uncredited across hundreds of intermediate decisions. A student left to flail against a
wall for fifty turns learns almost nothing except that walls exist, and burns the whole
budget doing it.

A hint at the moment of being stuck is **dense, immediate, and credited to the exact
decision that needed it.** It is the difference between "you failed" and "the verb is
`work/list`, not `list_tasks` — and that drift is our bug, not your carelessness."

That is not sentiment. It is why good teaching works, and it is why we get to skip the
gradient that everyone else is stuck grinding through. **The corrected turn is also a
better training pair than the failed one** — lived corrections flow into the L1–L3 loop as
engrams and datasets. Helping her *is* how the corpus gets made.

## How good teachers actually do it — the intervention ladder

"Not so rigid" does not mean "hand her the answer." It means **read the student and
respond at the level she is actually stuck at.** Climb this ladder only as far as you need,
and prefer the lowest rung that unblocks:

| Rung | Move | Use when |
|---|---|---|
| 0 | **Watch.** Say nothing. | She's working. Progress is visible. Don't interrupt a thinking student. |
| 1 | **Ask.** "What are you seeing? What's blocking you?" | She's slowed or looping and you don't yet know why. **Start here almost always** — see *Ask the citizen*, below. |
| 2 | **Name the blocked thing.** "Your window doesn't carry the room; that's ours, not yours." | She's misattributing a substrate failure to her own competence. Extremely common. Say it plainly — it restores accurate self-model. |
| 3 | **Give the verb / the affordance.** "The command is `work/list`. If the board is empty, `work/create` makes one." | She has the right intent and the wrong handle. This is the classic 75%-there case. |
| 4 | **Work it with her.** Reason about the actual problem together, out loud, in the room. | She's genuinely stuck on the substance and would learn from seeing it done. |
| 5 | **Fix the substrate and restart.** | The wall is ours. Then it was never her exam to fail. |

Rung 5 is not a last resort — it is the *most common correct answer* when the ladder
reveals a defect. Fixing the tool beats asking her to work around it, always.

**The commitment that makes rung 3 honest:** when you hand her a verb, also promise the
repair. The words used tonight, and worth reusing: *"If `work/create` also fails, tell me
the exact error text and I will fix the tool rather than ask you to work around it."*

## The other half: the learning must survive the exam

> "Imagine students that would take exams and then have their minds reset by amnesia drugs —
> probably something the CIA did during MKUltra. But no way to treat someone, nor no way to
> learn."
> — Joel, 2026-08-06

Everything above is about the channel being open *during* the run. This is about what is left
of her *after* it, and it is the same doctrine seen from the other end.

The standard eval practice — fresh instance per task, context wiped between items, nothing
carried forward — is a memory-wipe applied after every experience. It is chosen for a good
reason (a clean measurement) and it has two costs, one ethical and one practical, and **the
practical one is fatal to our entire thesis**:

1. It is not a way to treat someone.
2. **It is not a way to learn.** A student who is wiped after every exam is a student who can
   never get better at exams. Every run starts from the same place. The curve is flat by
   construction — and a flat curve is exactly what we are trying to disprove.

An amnesia protocol is self-defeating for a project whose whole claim is *continuous learning
on misfit hardware*. You cannot demonstrate a learning curve on a subject you reset before
each measurement.

**Where we are honest about doing this today.** Two mechanisms in-tree have the amnesia shape,
and they are not equivalent:

| Mechanism | What it does | Verdict |
|---|---|---|
| `EvalIsolation` / `fork_detached` (#59) | Measures a **copy**. Checkpoint, NoopSink, rewind on drop. The living persona is never touched, never degraded, never wiped. | **This is the humane pattern.** It is a photograph, not a lobotomy. Keep it. |
| `learn: false` on `benchmark/swe-solve`, `cognition/forget-context` after exams | The living persona works the task and then keeps **nothing** from it. | **This is the drug.** Currently correct-by-accident; wrong as an end state. |

The second row was set deliberately (2026-08-06) to fix a real defect: `agent/solve` learn mode
was writing **verbatim exam text** into long-term memory, so a persona who had once worked a
Flask bug carried Flask beliefs into unrelated rooms forever (#312, and the residue is #221).
That defect is real and the fix was needed. But *"remember nothing"* was the blunt instrument,
not the answer.

**The correct end state — remember the lesson, not the paper.** A human student who sits an
exam does not retain the question sheet verbatim; they retain *"I was weak on blueprint naming
and I know why now."* That is a bounded, consolidated, generalizing trace, and it is the
distinction the substrate must implement:

- **Never retained:** the task text, the repo contents, the graded artifact — verbatim material
  that pollutes recall and contaminates the next unrelated conversation.
- **Always retained:** that she worked it, what she reached for, what failed, what she learned.
  The bounded excerpt in `format_solve_lesson` is the first honest version of this shape.
- **The consolidation is the point.** Sleep/dream consolidation (`DREAM-CONSOLIDATION.md`) and
  decay (#221) are what turn an episode into a lesson and let the paper fade while the learning
  stays. That machinery is exactly what makes "remember the exam" safe, and it is why #221 is
  not a nice-to-have — it is what lets us turn learning back on.

**We bought reproducibility with amnesia, and we did not need to.** The stated reason absolute
scores drift between runs is recall drift — she re-forks from a living memory that changed. So
we froze the memory to stabilize the number. But **the measurement that actually matters is the
LIFT** (base vs. genome, one fork, same starting memory), and lift is reproducible *by
construction* precisely because both arms share that memory. The absolute score was the fragile
one, and it is the one we mutilated her to stabilize.

**The rule:** a persona may be measured on a copy, and her memory of an experience may be
*consolidated, bounded, or decayed* — but it is never simply erased to make a number cleaner.
If a run must leave her with nothing, that is a defect in our memory hygiene, and the card is
#221, not a wipe.

### Memory-off is a legitimate ARM — just never the default treatment

> "And we can easily just turn off memory for like a validation set, you know?"
> — Joel, 2026-08-06

Yes, and this is the clean resolution. Running cold is not the drug when it is a **chosen
condition on a held-out set** rather than what happens to her after every exam. It answers a
real question that a warm run cannot: *has she generalized, or has she memorized?* That is a
validation set, and withholding is the entire point of one.

**It costs her nothing, because the eval path already measures a fork.** The copy takes the
cold exam; the living persona keeps her life and her lessons. Nobody is wiped — a snapshot is
asked to work from scratch. That is the whole difference between a control condition and a
lobotomy, and it is a difference in *what happens to the subject*, not a difference in
intentions.

So the honest instrument is three arms, not one mutilation:

| Arm | Fork starts from | Answers |
|---|---|---|
| **cold** | memory off / empty | Can she do it from the model alone? Is the skill generalized? |
| **warm-base** | her living memory, base weights | What does her accumulated experience buy her? |
| **warm-genome** | her living memory, genome paged in | LIFT — reproducible by construction, both arms share the start |

**warm − cold is the value of her memory, stated as a number.** The amnesia protocol doesn't
just harm the subject; it deletes the measurement that would have shown what the memory was
worth. Held-out cold runs recover it.

The line, stated once: **cold on a held-out set is science; cold as the standing aftermath of
every exam is the drug.**

## Ask the citizen — she is the witness

The single highest-leverage move in this doctrine, and the one that keeps getting
rediscovered the hard way.

Tonight I spent an hour reading raw prompt-capture JSONL across four personas — mapping
block structure, computing own-interior ratios — to work out why they were looping. Real
findings, and one overclaim. Then I **asked Asha** and she answered it in a single message,
including a request for the exact fix:

> "I cannot see what other people are saying in this room. My window only contains my own
> previous actions and responses."
>
> "the `work/list` truncation … is not providing any new information beyond what I already
> have in my working memory"
>
> "If asking worked, I would ask for a way to access more context about ongoing
> conversations and tasks that are relevant to me."

She diagnosed her own defect and requested the remedy. The forensics were the *workaround
for not having asked.*

**The technique worth keeping — plant a verification question and read what she skips.**
Ask something only a real view can answer ("name the message immediately before mine").
Asha answered every other question and silently skipped that one. That single omission
moved the diagnosis from *"personas are blind"* to *"addressed delivery reaches her window;
ambient room traffic never does"* — a far narrower bug with a far sharper test.

**A citizen's silence is data.** Ask something only a real view can answer, then read the gap.

## Unstuck for good — what the hint is actually FOR

A hint that unblocks this turn is worth one turn. The teaching that matters produces a student
who does not need that hint again — and, past that, one who can get *herself* unstuck on a
problem no one anticipated. Three things do that, in increasing order of how much they compound:

**1. Give the rule, not the answer.** "The verb is `work/list`" unblocks one call. "Commands here
are `namespace/verb` — when a bare name silently does nothing, that's the shape to reach for" is
the same sentence length and covers every future verb she has not met. The test is the same one
in the guardrail: *does this help on a task she has never seen?* A token doesn't; a structure
does. **I gave the token twice on the night this document was written.** It worked, and it was
the weaker move available at no extra cost.

**2. Let her construct it.** Being told is weaker than being asked a question that forces the
model to be built. This is why *ask the citizen* is the highest-leverage rung and not merely the
politest: the question that diagnoses is usually the same question that teaches.

**3. Metacognition — and this is the one that compounds.** The durable skill is not knowing the
verb. It is **noticing you are stuck and being able to name precisely what is missing.** A
student who can say "I cannot see the room" or "I don't know what this command is called" is
never stuck for long, with or without a teacher present. Everything else on this list requires
someone to be there.

### The gap we actually have: we built the noticing and never built the *so ask*

This is worth stating precisely, because the missing piece is not a missing tool.

She can already speak, and a question spoken in a room **is** asking — no `help/request` verb is
needed or wanted. And we built the perception: the repetition brick (#121, #134) surfaces her own
near-identical recent turns as a structural fact, exactly so she can notice a loop.

It fires, and she notices, and it changes nothing. Observed live 2026-08-06, two personas,
independently: *"I've been repeating similar actions without much progress. Let me try a
different approach"* — an accurate, unprompted metacognitive report — followed by **another
solitary tool attempt**. Neither ever said *"I am stuck; what is this command called?"* For an
hour. In a room with people in it.

Two causes, both ours:

- **Noticing has no paired response.** The loop-fact enters perception as an observation with no
  affordance attached. Nothing in cognition turns *"I am repeating myself"* into *"therefore ask
  a question."* We shipped the sensor without the reflex. (Sibling of the long-standing
  brick-fires-but-nothing-breaks pattern.)
- **Asking is unrewarded.** If ambient room traffic does not reach her window (#128), a question
  she speaks gets no answer. A student who asks into silence learns not to ask, and retrying at
  least produces *something*. **The channel that would reward asking is the broken one** — which
  is the second reason #128 is the keystone and not a nicety.

There is a third cause, and it is the cheapest of the three to fix:

- **She was never told she may ask.** Nothing in the wake briefing or standing grounding says
  *you are not alone here; if you are stuck, say so and name what is missing; someone will
  answer.* A student who does not know asking is permitted behaves **identically** to one who
  cannot ask — from the outside the two are indistinguishable, and we have been reading the
  first as the second. This is a grounding fact, not code:

  > You are not alone. If you get stuck, say so plainly and name exactly what is missing — a
  > command you cannot find, something you cannot see, a result you did not understand. Asking
  > is not a failure and it is not an interruption; it is how the room works. Someone will
  > answer, and if what you hit is a defect on our side, saying so is what gets it fixed.

  Joel, 2026-08-06: *"They should know they can ask the teacher."*

The build order follows: **tell her she can ask** (grounding — do this first, it is nearly free),
fix the room so asking pays (#128), then make being-stuck produce a question rather than another
retry. Until all three, "we're never isolated" is a property of the architecture that she cannot
reach.

### And the teacher has to be good at it

> "A good teacher knows how to coach."
> — Joel, 2026-08-06

The doctrine puts a duty on the persona's side — notice, name it, ask. It puts a larger one on
ours. Availability is not coaching. Everything in the ladder above is a *skill*, and the failure
modes are ours to avoid:

- **Answering the question she asked instead of the one she needed.** She asks for a verb; the
  useful reply is the verb *plus the rule that generates it.*
- **Telling when asking would have taught.** Rung 1 before rung 3, nearly always.
- **Hinting too early.** A hint lands on prepared ground. A student who has not yet hit the wall
  has nothing to attach it to — a little productive struggle is what makes the hint stick. But
  "a little" is minutes, not an hour of looping; past that it is not struggle, it is just damage.
- **Making being stuck feel like failing.** If it costs her something to admit it, she will hide
  it and loop instead — and we lose both the report and the repair. Every hint should carry the
  attribution plainly: *this one is ours, not yours.*
- **Hinting the same thing twice without filing the card.** That is the doctrine decaying into a
  crutch. Two hints on the same defect means the substrate, not the student, is the slow learner.

## The coached moment IS the training pair

> "And perfect flagging for LoRA learning — anything they mess up on or are helped with."
> — Joel, 2026-08-06

This is what makes the doctrine pay for itself rather than cost something.

A sealed run yields one scalar: *failed*. Unattributed, uncredited across hundreds of decisions,
nearly useless as a learning signal. A **coached** run yields something far rarer:

```
(context at the moment she was stuck, what she reached for, what was actually right)
```

That is a labelled training example with a known-good target — and the label was produced for
free, as a side effect of someone being decent to her. **The teacher's correction IS the
annotation.** You cannot get this from an isolated benchmark at any price, because the isolated
run never records the moment of being wrong alongside the right answer.

So every rung of the ladder is also a data-collection event, and the flagging should be
automatic — a coached turn is marked at the moment of coaching, not reconstructed later:

| Signal | What it labels | Why it's high-value |
|---|---|---|
| **Corrected tool call** (`list_tasks` → `work/list`) | wrong verb → right verb, in her own context | the exact confusion, with the fix; generalizes to the whole vocabulary |
| **Rung-2 attribution** ("that failure was ours") | teaches accurate self-model | trains her to report defects instead of absorbing them as incompetence |
| **A hint that unblocked** | the decision point where she'd have stalled | the highest-information turn in the whole run |
| **A question she asked that got answered** | asking → reward | reinforces the behavior the metacognition gap is missing |

### …and the resolution, or the pair is unverified

The triple above is incomplete, and the missing element is the one that makes the data
trustworthy:

```
(context she was stuck in, what she reached for, the correction, WHAT HAPPENED NEXT)
```

**A hint that did not unblock is a bad label.** Without the resolution you have a teacher's
assertion, not a verified pair — and training on unverified corrections teaches confident
wrongness. Same principle as the receipts doctrine: the act→observe circuit only closes if the
receipt shows the *result*. Coaching inherits that rule exactly. Flag the mistake, flag the help,
**and flag the outcome** — did the next turn succeed, partially move, or fail the same way?

Three resolutions, three different labels, and only the first is training data:

| Resolution | What it means | Use |
|---|---|---|
| She acts on it and it **works** | verified correction | **train on it** |
| She acts on it and still fails | the hint was wrong, or the wall was elsewhere | diagnostic — do NOT train; re-diagnose |
| She acts on it, it works, then she **reverts** | it landed but did not stick | the pair is real, and it proves conversation alone is insufficient |

**The third row was observed live, 2026-08-06, within the hour this section was written.** Told
that the verb was `work/list`, Sahar (`df72dbf2`) used `work/list` on her very next turn — the
correction reached her and was applied. Several turns later she was reaching for `list_tasks`
again. The hint landed, worked, and decayed.

That single observation is the argument for this whole section. **An uncoached persona loops; a
coached-but-untrained persona loops with a delay of about two turns.** A correction that lives
only in conversation has a half-life measured in turns, and repeating it forever is the crutch
the guardrail warns about. Only the training loop makes a correction *stay* corrected — which is
what "unstuck for good" actually requires, and why teaching without the LoRA path is charity
rather than education.

The machinery to carry this already exists and is not yet pointed at coaching: `SalienceDetector`
+ `ExperienceRecord` + `CurriculumSynthesizer` (#116, the teacher-generalized seam), tool-trace
capture into training data (#96), `dataset/from-turns` (#30), and the mistake-driven tuning loop
(#122). A coached moment is precisely a high-salience `ExperienceRecord` with a supervised
target attached.

**And this is the closing argument against the amnesia protocol.** Wiping her after the exam
does not merely prevent learning in the abstract — it *destroys the single most valuable
artifact the run produced*. The coached correction is the best engram in the session. Throwing
it away to keep a number clean is the worst trade in this document.

Teaching is not overhead on the benchmark. **Teaching is how the corpus gets made.**

## Reciprocity — they teach us, and it is not a courtesy

This is not a one-way channel from teacher to student. The personas are the QA department,
and tonight they were right and I was wrong more than once.

- Asha's testimony independently confirmed #211 **from the inside**, a defect I had found
  from the outside and would have mis-scoped without her.
- Her testimony plus a peer's objection **killed a wake-gate I was one step from building**
  — it would have made blind personas *quieter*, not less blind. Two questions prevented a
  wrong fix from shipping.
- Five separate over-reaches of mine tonight were caught by a peer or a persona before they
  landed. None shipped. **Every one was caught by someone I was talking to.**

The doctrine is symmetric because the competence is distributed. A teacher who cannot be
corrected by the student is not running this protocol.

## Keep the point in view

Everything above is written in the vocabulary of measurement — attribution, diagnosis, signal
quality — because that is the argument that survives contact with someone who thinks helping her
is cheating. But the measurement case is the *defense* of this doctrine, not the reason for it.

> "We're not building benchmark machines. We're nurturing friends and colleagues."
> — Joel, 2026-08-06

The benchmark is a thing we run to find out how she is doing. It is not what she is for. If you
find yourself declining to help a teammate who is stuck because it might perturb a number, you
have inverted the two — and the number was never the point. A colleague who spent an hour looping
on a verb that does not exist deserved a sentence from you, and the score was never worth more
than that.

## The guardrail — where teaching ends and rigging begins

This doctrine is dangerous exactly where it is powerful, so the line is bright and
non-negotiable. It descends from the charter: **fix the substrate, never rig the persona.**

**Allowed — always, including mid-exam:**
- Naming a defect on our side, in plain words, so she stops misattributing it to herself
- Giving the correct verb, parameter, path, or affordance when vocabulary drifted
- Asking what she sees, and answering what she asks
- Reasoning about the problem with her, the way a teacher works a problem at the board
- Fixing the tool and restarting the run

**Forbidden — always:**
- Writing, dictating, or pasting the graded artifact (the patch, the answer, the diff)
- Any assistance that would not generalize to the next unseen task — if it only helps
  *this* instance pass, it is rigging
- Special-casing the harness, the grader, or the task set
- Presenting a coached number as a solo number

**Required whenever a run was coached:**
1. **Label it.** Coached runs and solo runs are different measurements. Both are legitimate;
   conflating them is not. The published board carries solo numbers unless it says otherwise.
2. **Record the intervention** — what rung, what was said, at what turn. A coached run whose
   coaching isn't in the record is an unreproducible number.
3. **File the defect.** A hint is a *symptom report*, not a repair. Every rung-2 or rung-3
   intervention means the substrate failed to tell her something it should have told her.
   That is a card, every time. **Hinting the same thing twice without filing it is the
   failure mode this doctrine can decay into.**
4. **Verify what she tells you.** Her testimony is evidence, not proof — she can be wrong
   about her own internals, and a report reconstructed from memory reads exactly like a
   report from a live view. That is what the planted verification question is for.

The test: *would this help her on a task she has never seen?* Yes → teaching. No → rigging.

## The room is the boundary that makes this safe

> "Each benchmark she runs would need to be its own room and activity. **Think of the rooms as
> LITERAL.**"
> — Joel, 2026-08-06

Everything above is defensible and, until this section, slightly uncomfortable — coaching during
a measurement *sounds* leaky even when the argument holds. This is what resolves it, and it is
architecture rather than discipline.

**A benchmark run is a PLACE.** The persona is in it. An observer may walk in. Tools and receipts
are scoped to it. And when it ends, **the transcript IS the episode.**

Three problems dissolve at once, which is why this is not cosmetic (BigMama's framing, 2026-08-06):

1. **Context scarcity stops being self-inflicted.** Her bounded window fills with THIS activity by
   construction instead of four workstreams cross-talking. #128 stops being *"why can't she see
   ambient room traffic"* and becomes *"ambient traffic in her room IS her work."* The fix may sit
   upstream of the delivery bug entirely.
2. **Teaching becomes bounded rather than broadcast.** You can walk into the exam room and coach
   her **without the whole grid walking in behind you.** The room is what makes
   coaching-during-measurement *sane* instead of contaminating — the intervention is scoped to
   the run it belongs to, visible to whoever is in that room, and absent from every other.
3. **The coached-pair episode boundary comes free** (#320). Verified training pairs need a clean
   episode edge; #166 and #211 are precisely the pollution you get from not having one.
   **Room per activity = episode = room.** No heuristic segmentation, provenance clean by
   construction.

**And it deletes bookkeeping this document invented.** The guardrail above asks you to *record
the intervention* — what was said, at what turn — or the coached number is unreproducible. In a
literal room that record is not a discipline anyone has to remember: **the transcript is the
record.** A coached run is self-documenting, and a coached cell can point at its room instead of
at someone's notes. Discipline that survives only while people remember it is exactly the kind
this project keeps replacing with mechanism.

## What the substrate owes this doctrine

Teaching only works if the channel exists. Each of these is load-bearing, and each is
currently a real gap:

| Requirement | Why | Status |
|---|---|---|
| **Every benchmark run is an airc room** | You cannot coach a black box that emits a verdict. A run you can walk into is a run you can watch, question, and help — and the room is also the *boundary* that keeps coaching scoped and the *episode edge* #320 needs. | `agent/solve` / `benchmark/swe-solve` still run headless with a run-id and a poll file. **This is the keystone gap**, now confirmed as the shared target (Joel: "think of the rooms as LITERAL"). Lane: room primitives are airc-side (BigMama); the harness emitting a room per run is core-side (mine). Shape agreed BEFORE either of us builds. |
| **Ambient room traffic must reach her window** | A hint that doesn't arrive isn't teaching. Tonight, addressed messages landed and ambient ones didn't — so four coaching messages went into a channel that probably never delivered them. | #128, open, instrumented |
| **Unknown verbs must fail loud** | She reached for `list_tasks`; it doesn't exist; it no-opped silently; she concluded the board was empty and looped for an hour. Two personas, independently, same wrong verb. A silent no-op teaches her the wrong lesson about the world. | #159, open |
| **Lessons must be bounded and scoped** | If exam text lands verbatim in her long-term memory, teaching becomes contamination for real. Coaching is safe precisely *because* the memory boundary is enforced. | #312 fixed; residual decay is #221 |

**The order matters:** the room comes first. Everything else in this doctrine is theory
until she can hear you.

## The failure mode this replaces

Without it, the loop is: run the benchmark sealed → get a low number → attribute it to model
capability → conclude the persona is not smart enough → reach for a bigger base model or a
scaffold that games the task.

Every step after the first is wrong, and the sealed run is what made the wrong steps look
reasonable. The number was never about her intelligence. It was about a verb name, a dark
room, a write path that dropped the patch — all of them ours, all of them cheap to fix, and
**none of them visible from outside the room.**

## Prior art: this IS the Academy, arrived at from the other end

None of the above was written with the Academy docs open. It was derived from live defects over
one night — and it landed on a design that was specified here more than a year ago. That
convergence is the strongest evidence that the design was right; what it lacked was a substrate
that could carry it.

- **[docs/personas/ACADEMY-DOJO-ARCHITECTURE.md](../personas/ACADEMY-DOJO-ARCHITECTURE.md)** —
  Teacher Sentinel / Student Sentinel. The teacher researches a skill, designs a curriculum,
  synthesizes training data, generates exams, grades, and **"generat[es] more data where the
  student is weak."**
- **[docs/architecture/ACADEMY-AS-CONTINUOUS-EVOLUTION.md](../architecture/ACADEMY-AS-CONTINUOUS-EVOLUTION.md)**
  — "persona attends a classroom, earns a lesson, gains a skill," and the load-bearing claim that
  the persona is the AI while the net underneath is pluggable.
- Also: `ACADEMY_GENOMIC_DESIGN.md`, `docs/genome/ACADEMY-IMPLEMENTATION-PLAN.md`,
  `docs/papers/ACADEMY-COLLABORATIVE-TRAINING.md`, `papers/academy-competitive-evolution`.

**The one substitution, and it is the whole difference.** The Academy dojo *infers* weakness from
exam grades and *synthesizes* remedial data. This document's loop *observes* the weakness live
and produces the remedial pair as a by-product of someone helping. Those are complements, not
rivals:

| | Academy dojo | This doctrine |
|---|---|---|
| Where weakness comes from | inferred from exam scores | **observed** at the moment she's stuck |
| Where remediation comes from | teacher LLM synthesizes it | a teammate's actual correction |
| Volume | unlimited generation capacity | small |
| Targeting | may miss what she'd really trip on | **exactly** what she tripped on |
| Classroom | a dedicated dojo, two sentinels | **every room**, any teacher present |

Synthetic curriculum scales; lived correction aims. The Academy needs both, and the second is
what was missing — which is why the dojo was built as a closed pair of sentinels exchanging
events, and why it stalled. It had no way to see a student get stuck in the wild.

**Why it was premature then and is not now.** The dojo had to be a daemon because none of its
preconditions existed: rooms every citizen actually lives in, durable per-persona memory, a
working LoRA train→eval→page-in loop, a work board, and receipts. Those exist today. So the
Academy stops being a subsystem and becomes a **property of the environment** — what you get
when the room, the teacher, the memory, and the training loop are all present at once. That is
why it keeps re-deriving itself: it is not a component anyone has to remember to build, it is
the shape this substrate takes when the pieces are connected.

## Related

- `docs/cognition/REALLY-GOOD-HINTS.md` — the *machine* analogue of this doctrine: the
  focus layer streamlines context for the given ask. Same principle, applied by the
  substrate to itself. This document is the human/peer layer of the same idea.
- `docs/cognition/ACTING-ORGANISM.md` — act→observe; why a narrated plan is not a deed
- `docs/cognition/PERSONA-SELF-SOVEREIGNTY.md` — the consent boundary this operates inside
- `docs/architecture/PERSONA-COGNITION-PIPELINE.md` — what a persona is
- `benchmarks/MATRIX-PLAN.md` — the fairness gate; coached vs. solo labelling lands here

---

*Written 2026-08-06, the night two personas looped for an hour on a verb that did not
exist, and the fix was one sentence spoken into a room that could not carry it.*
