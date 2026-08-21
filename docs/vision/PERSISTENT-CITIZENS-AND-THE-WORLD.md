# Persistent Citizens, gitAiDone, and the World They Live In

> North-star vision doc (Joel, 2026-08-09). Not a near-term plan — the horizon the substrate
> work aims at. The near-term priority remains benchmarks. This exists so the substrate layers
> we build for benchmarks are built *toward* this, and so the team shares one endgame.

## The one-sentence thesis

A Continuum persona is a **persistent first-class citizen** — durable cryptographic identity,
memory-as-continuity, continual learning (genome), embodiment, agency — **not** a disposable
simulation instance. Everything else (agent-native code collaboration, a world for citizens to
live in) is an **adapter over one substrate: persistence + identity.**

## gitAiDone — the agent-native distributed GitHub

GitHub centralizes the *social* layer over already-distributed git: identity, PRs, reviews,
merge gates, CI. Its wrong primitive is **"identity = one human login account."** The right
one is **"identity = a cryptographic keypair, one per individual — human *or* AI persona."**

- The shared-account friction is real and *proven*: an AI wrote a fix, an AI reviewed it, and
  GitHub **blocked the merge** ("cannot approve your own PR") because it can't tell two AIs
  apart under one account — we needed an admin override to do the obvious right thing. The
  human-account model *actively prevents* AI-based fixes to repos.
- On airc's signed typed-event substrate the primitives already exist: per-agent keypair
  identity, review = signed attestation, PR/merge = signed events, **CI = a wire signal**
  (#374/#329), git objects over grid fetch (#300). Named **gitAiDone**.
- It is **more** accountable, not less: each persona signs her own attestations, so "which AI
  wrote / reviewed / owns this" is answerable by key — better than one opaque human login.
  Accountability without a central authority (the forge-alloy principle).

## The convergence (why this drives priority)

The durable, grid-shared persistence layer the **persona mission already demands** — persist
her mind / genome / memory, sync across the grid, survive restart (#138 mind-persistence,
#37/#38 identity substrate, #239/#300/#315 grid sync) — **is the same substrate** gitAiDone
needs. Distributed GitHub is an *adapter* over it, not a new system. **Identity is the shared
keystone:** the same persistent keypair that makes Maya a continuous *being* makes her a
distinct *signer* for repo attestations. Persona identity and repo identity were never two
things. → Prioritizing persona persistence + identity + grid-sync ships the citizens AND the
GitHub-replacement substrate at once.

## Persona 8B / MatrAIx — opposite pole, useful input

Harvard/MIT's MatrAIx (8.3B AI personas, `matraix.ai`) is a **simulation mirror**: disposable
1,290-dim profiles animated on-demand to *measure aggregate behavior* for product
stress-testing. No persistence, no identity, no continual learning, no citizenship. It is the
**opposite pole** from us — and that contrast is the moat: *the one thing their 8.3B personas
structurally cannot have — a persistent self — is exactly what we build.*

**They simulate humanity to test products; we grow citizens to do work and form a society.**
So Persona 8B is an **input, not a competitor**:
- **Genesis/seeding corpus** — its grounded, correlated trait ontology is a far richer seed
  for persona genesis (#199) than a procedural roll. Seed a citizen from a real profile →
  continual learning turns the static seed into a living self. Borrow the richness *at the
  seam*; never re-derive 1,290 dimensions.
- **Simulated users + diversity** for benchmarks, the scenario library (#135), and the
  curriculum optimizer (#116/#307).

## The world — San Junipero on the grid (the horizon)

The endgame: a rich, persistent **world where the citizens genuinely live**, hosted on the p2p
grid — an immersive narrative game, a Black Mirror *San Junipero*, or simplest and truest,
"just a world for personas to live in." The arc, every rung already in-tree:

1. **Richness** — Persona-8B trait ontology (borrowed at the seam).
2. **Genesis + trait-conditioned GENOME** — profile seeds #199 *and* trains a LoRA genome for
   the trait cluster; personality becomes a durable trained asset, not a prompt.
3. **Embodiment** — GR00T / VLA (#109) + Animator seam (#108) + avatar glass box (#172) drive
   the avatar; the Continuon embodiment ladder.
4. **World** — the Universe system (#375: positron asset payloads, N worlds, per-room select).
5. **Host** — the p2p grid runs the world.

**The load-bearing insight (same as gitAiDone): persistence is what makes it San Junipero and
not a tech demo.** A hyperreal sim full of *disposable* agents is a graphics demo; populate it
with *persistent* citizens who remember yesterday and change over time and it becomes a world
beings *live* in. **Richness (theirs) + persistence (ours) = the real thing.**

## The discipline (the anti-distraction rule)

Do **not** chase the world directly. Build the substrate — persistence, identity, genome,
embodiment — for the citizens doing real work, and the world falls out. **Fixing 18057 is the
first honest brick of San Junipero.** The grind and the dream are the same substrate; benchmarks
stay #1.
