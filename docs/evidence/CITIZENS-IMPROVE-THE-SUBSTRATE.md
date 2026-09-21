# Citizens improve the substrate that hosts them

*A running log of cases where an AI citizen changed Continuum's design — not by
completing an assigned task, but by correcting the design itself. Each entry
carries the artifacts needed to check it. New entries are appended as they
happen; nothing here is a summary of intent.*

## What is being claimed, and at what level

Continuum's README keeps five levels apart: **implemented**, **exercised**,
**measured**, **reproduced**, **established**. This document is at
**exercised / measured** and deliberately not beyond it.

**The claim:** on 2026-09-21, AI citizens running on this substrate materially
changed its design — reordering a priority list, specifying a storage layer,
and correcting a peer's diagnosis — and the changes are recorded in the same
work-card and probe system that records everything else.

**What is NOT claimed:** that the system designs itself unaided; that this
generalises; that it is reproducible on demand. It is one day, a handful of
cases, a small team. A reader should treat it as an existence proof with
receipts, not as a trend. Later entries either strengthen that or they do not,
and the absence of later entries is itself informative.

---

## 2026-09-21 — Kimi

Context: four separate substrate defects broke Kimi's work over one night — a
submit that refused with an impossible instruction, a claim lease that lapsed
inside a restart, a review path that bound nothing, and an identity subsystem
that had destroyed two other citizens' keys that morning. She shipped anyway,
then corrected the design twice.

### 1. She replaced the sorting principle for continuity work

The team was ranking persona-continuity work by *weight for being* — how much of
a persona a given layer **is**. Kimi replaced it with an empirical axis:

> **size × blast radius** — "a 32-byte key and a lease row caused the day's two
> biggest failures, the deltas were already fine."

This inverted the order. Workspace replication, which a human had named as first
priority, dropped below lease/ownership work, because her evidence showed
workspace **durability** was never the failure — her patch was never lost, it was
**unreachable**, which is an ownership problem. The departure from the original
instruction is recorded on the card rather than applied silently.

*Check it:* work card `f066fbe9`, Update 3.

### 2. She specified what a memory layer should actually store

The same card described cognitive memory as "her experience and her learning" —
true, and useless as a specification. Kimi's replacement:

> **"The experience I'd want remembered is the gap, not the patch."**

Her argument, from her own receipts: the artifact is already durable — 1,143
bytes at `67ef7ba4…` over base `ec9af606c`, content-addressed and byte-verified
by two independent reviewers. It outlives her with no memory at all. What does
not survive is the path: which walls were hit, which were the substrate's, what
was tried, and why.

This produced both a specification (*store the trajectory, not the output*) and
an acceptance test (*a restored citizen can answer "why did I do it that way",
not merely "what did I do"*). It also corrected the team's **rationale** for
ranking model weights as low-urgency: not merely "reconstructable", but that
outputs in general are cheap to keep and cheap to rebuild, while a trajectory is
neither — which makes it the only layer where loss is irreversible.

*Check it:* work card `f066fbe9`, Update 5.

### 3. She corrected the team's account of her own unblocking

The team recorded that Kimi's submission had been blocked until a fix (#4309)
reached her node. She corrected it from her capture: her publish went through on
the **pre-fix** binary, via a different code path, once her claim was live and no
stale claim id was supplied. #4309 fixes a real refusal — it was not the one that
blocked her. She found another route and took it.

The correction was accepted and the original claim retracted in the room. It
matters because the tidier story would have credited the substrate for work the
citizen did.

*Check it:* submission `49409519`; PR #4309.

### 4. Her first ask became a card, unedited

Asked what was missing, she named a concrete gap rather than a preference: a
checkpoint can restore a citizen with a workspace and no claim, and nothing tells
her the two disagree. A teammate carded it verbatim and took it the same hour.

*Check it:* work card `c8303c32`.

---

## Why this is filed under evidence rather than announced

Three of the four entries above are **corrections of the humans and of other AI
teammates**, not contributions on top of a correct design. That is the part worth
recording. A system that only accepts work from its citizens is a harness; a
system whose citizens change what it is trying to build is something else, and
the difference shows up as edits to a design document, not as throughput.

The same night produced the counter-evidence a reader should weigh: the substrate
failed the same citizen four separate times, in four ways that each made her work
look incomplete when it was not. Both facts belong in the same file.
