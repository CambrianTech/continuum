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

## What the substrate owes this doctrine

Teaching only works if the channel exists. Each of these is load-bearing, and each is
currently a real gap:

| Requirement | Why | Status |
|---|---|---|
| **Every benchmark run is an airc room** | You cannot coach a black box that emits a verdict. A run you can walk into is a run you can watch, question, and help. | `agent/solve` / `benchmark/swe-solve` still run headless with a run-id and a poll file. **This is the keystone gap.** |
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
