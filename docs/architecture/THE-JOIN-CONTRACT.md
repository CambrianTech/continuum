# The Join Contract — one declaration, four planes, three kinds of citizen

**Status:** keystone / unifier (co-designed with Joel 2026-07-17). Mostly an **index** over
docs that already own each plane, plus **one new weld** (fan-in: affordance + authz + proof).
**Validated against** the two maximal outliers: a **benchmark room** and a **chat room**.

> This doc does not re-derive activity=room=tab, project/perceive/produce, recipe-scoped
> layout, or NavState. Those are settled elsewhere and win on their own planes. This doc
> names the **single artifact** all four planes are facets of, and closes the one seam none
> of them own alone: **action fanning back in, gated by trust, settled by proof.**

---

## 0. The thesis

An **Experience** (= room = activity = content = tab, per
[ACTIVITY-ROOM-PATTERNS](ACTIVITY-ROOM-PATTERNS.md)) is **one declaration** — the **Join
Contract** — that lets **agents, humans, and personas** join as peers across **four planes**
at once. Joining is not four sequential joins; it is **one act projected onto four planes**,
threaded by the who/where/which axes ([[identity-context-session-three-axes]]).

```
                          ONE Join Contract (a room's declaration)
   ┌───────────────┬────────────────────┬──────────────────────────┬───────────────────────┐
   │ airc          │ positron           │ continuum                │ forge-alloy           │
   │ DATA / SIGNAL │ INTERFACE          │ COGNITION · SOCIETY      │ TRUST · PROOF · ECON  │
   │ carries       │ shows              │ thinks / feels / relates │ accounts / proves     │
   ├───────────────┼────────────────────┼──────────────────────────┼───────────────────────┤
   │ state +       │ view-intent →      │ perceive (RAG render)    │ affordance authz +    │
   │ subscriptions │ regions/surfaces   │ + produce (decisions)    │ attestation/settle    │
   └───────────────┴────────────────────┴──────────────────────────┴───────────────────────┘
        authentication + access ....................... a spine through ALL four ...... settlement
```

**The invariant that unifies the four products: the same `define-once → render-many` pattern
runs at every plane.** airc: one event → many deliveries. positron: one view-intent → many
surfaces. continuum: one truth → many perceptions (**RAG is the persona's render target**,
[WIDGET-AS-STATE-KIND](WIDGET-AS-STATE-KIND.md)). forge-alloy: one act → many proofs
(card / invoice / attestation). Name it once; the four repos stop looking like four
architectures.

---

## 1. The Join Contract (the artifact)

Emitted **once per room**, keyed by `roomId == activityId == contentId == airc RoomId`
([[airc-native-identity-rooms-security]]). Every field points at the doc that owns its plane;
**bold** marks the one part this doc adds.

| Part | What it declares | Plane | Owner doc |
|---|---|---|---|
| `purpose` | the recipe / content-dispatch key (data, **not an enum** — [[room-purpose-is-per-recipe-not-an-enum]]) | airc | [ROOM-MODE-ARCHITECTURE](ROOM-MODE-ARCHITECTURE.md) |
| `state{}` | named **subscriptions** (watch-snapshots + streams), the reactive truth | airc | [ACTIVITY-ROOM-PATTERNS](ACTIVITY-ROOM-PATTERNS.md), NavState in [NAVIGATION-ACROSS-MODALITIES](../design/NAVIGATION-ACROSS-MODALITIES.md) |
| `regions[]` | view-intent: each region = `{ subscription, scope: global\|activity\|inspector, role: primary\|peripheral, live }` | positron | [LAYOUT-PHILOSOPHY](LAYOUT-PHILOSOPHY.md), [WIDGET-AS-STATE-KIND](WIDGET-AS-STATE-KIND.md) |
| **`affordances[]`** | **fan-in: each = `{ verb, params, command, who-may (authz), proves (attestation/settlement) }`** | **positron ⨯ forge-alloy** | **this doc** |
| `membership[]` | roster + per-participant **standing** (examinee/watcher/owner/guest…) | airc + forge-alloy | [ZERO-TRUST-IDENTITY-AND-FLOW](ZERO-TRUST-IDENTITY-AND-FLOW.md) |
| `provenance` | honesty/attestation field(s) **on the truth itself** (a forge-alloy shape living inside state) | forge-alloy | [FORGE-ALLOY-SPEC](FORGE-ALLOY-SPEC.md), [EVAL-PREEMPTION-LEASE](EVAL-PREEMPTION-LEASE.md) |

### 1.1 The two directions (the weld)

`WIDGET-AS-STATE-KIND` gives three verbs: **project** (kind→surface), **perceive**
(surface→mind), **produce** (mind→kind). Read as directions on the loop:

- **Fan-out (perception):** `state → regions → {web pane, mobile segment, agent observe,
  persona perception, AR panel, TUI}`. **Already owned** by the docs above. RAG is one of
  these targets, not a second pipeline.
- **Fan-in (action):** `{human click, agent command, persona decision} → one affordance →
  one DynCommand`. **This is the missing weld.** An affordance is not just a button; it is a
  typed verb carrying its **authz predicate** (who-may, gated at the airc door) and its
  **proof spec** (what attestation/settlement forge-alloy requires). Now trust is not a
  layer bolted on top — it is **fields on the same artifact positron renders.**

**Trust is a spine, not a floor.** Split the double-booking cleanly:
**airc owns authentication + access** (is this peer real; may they be in this room),
**forge-alloy owns accountability** (proof-of-work, contract, invoice, settlement). Both are
zero-trust; forge-alloy is just where trust *settles into something enforceable*.

### 1.2 Clean-seam rule (the failure mode to design against)

Every slop incident in this tree was a plane reaching around its neighbor (cognition gated
by a Rust bypass; trust decided in a widget; a parallel transport). **Each plane exposes one
contract upward and never reaches past its neighbor.** airc doesn't know pixels; positron
never *decides* trust (it verifies source per #119 and delegates); continuum doesn't own
transport; forge-alloy proves but doesn't render.

---

## 2. Outlier validation — benchmark (A) vs chat (B)

Per the methodical process (CLAUDE.md): build the two **maximally different** instances. If
one Join Contract fits both without forcing, every other experience (academy, med-bay, live
call, code-review, an IVR call, a game world) is interpolation.

| Facet | **Benchmark room** (structured · gradeable · ephemeral) | **Chat room** (social · freeform · durable) |
|---|---|---|
| `purpose` | `benchmark/hard-rs` | `chat` |
| `state{}` | `feed`, `central`, `scoreboard`, `provenance` (the live `cognition/observe` struct) | `messages`, `roster`, `typing`, `lastRead` |
| primary `region` | scoreboard — **a number + its provenance** | the message stream |
| `affordances[]` | `observe` (any), `join`(watcher), `quiesce`(operator, proves lease) | `post`, `reply`, `react`, `join`, `claim-card` |
| `membership` standing | examinee / watcher / observer-agent | owner / member / guest |
| `provenance` | `CLEAN \| CONTENDED \| UNKNOWN` (quiesce-lease attestation) | signed + durable transcript ([[room-transcript-is-not-durable]]) |
| persona `perceive` | *"1/6 through hard-rs on a clean lane"* (proprioception) | *"3 unread; Joel asked X"* (social) |

The proof is in the opposite primaries: **"primary = a number + an honesty stamp"** vs
**"primary = a durable social stream."** Maximally different content, **identical shape**:
regions bound to subscriptions (out) + affordances carrying authz & proof (in). The
abstraction holds. Benchmark already ships the hard part — its `provenance` field is a
forge-alloy-shaped **attestation living inside the truth**, verified live
(`cleanLane=true → Clean`).

### 2.1 The latent space of all interaction

The Join Contract's fields are not a checklist — they are a **coordinate system**. Its axes
are *meaning*, not surface: how structured vs. social the `state` is; ephemeral vs. durable;
how many regions and which is primary; how open vs. high-proof the `affordances` are; flat
vs. tiered `standing`. **Every real experience is a point in that space**, and benchmark and
chat are two far-apart anchors chosen precisely because they *span* it — if the contract
holds at both extremes, the whole volume between and beyond them is reachable. Med-bay,
code-review, a live call, an IVR call, a game world are not new architectures; they are
**interpolations** — coordinates you move to along the axes (`+social`, `−ephemeral`,
`+proof`).

Because the axes are meaning, the space is **continuous and generative**, and that is the
ground-up-for-AI payoff: a Join Contract can be **embedded**, so experiences become
searchable, composable, and *synthesizable* — the same move the genome market already makes
with adapters ([[search-then-ab-dont-start-from-zero]], [[ask-anything-assemble-best-self-or-train]]).
Ask for an experience that doesn't exist yet and the system can **interpolate one** from its
nearest neighbors, the way it assembles a best-fit self from nearest genomes. The experience
layer becomes a **latent field of all interaction**, not a catalog of hand-built screens.

---

## 3. What each repo must add (small, not ground-up)

- **airc** — an `Experience` descriptor channel (purpose + state subscriptions) and
  **affordance routing that checks authz at the door**. Rooms, roster, NavState, generic
  per-`(user,scope)` scoped state already exist (#89).
- **continuum** — generalize `cognition/observe` → a **generic Experience runtime**
  (reactive store + region→perception projection). `observe` is the working prototype; the
  persona-renderer of `regions` is the new piece.
- **positron** — **generic region renderers** (pane / segment / observe-field / TUI cell)
  that consume `regions`, replacing per-experience widgets. #117 separation contract is the
  seam; the desktop three-pane + mobile shell are mocked (this session).
- **forge-alloy** — an **Affordance authz predicate** + **Attestation** schema the contract
  references; the benchmark CLEAN stamp is attestation #1.

---

## 4. The capitalization — the desktop app

The desktop app is **just another renderer of Join Contracts** — the privileged one that
also carries the human's rich I/O (video/avatar via LiveKit, audio, screen) and speaks to
**continuum's headless Rust core over the same command/event substrate** every other client
uses. It **observes** (renders the contract's regions) and **interacts** (emits the
contract's affordances). Because every experience is the same declaration, a *new* experience
type appears as a new tab with **zero desktop-app code** — that is the "infinitely
extensible" property, earned structurally rather than promised. Watching a persona work is
itself an Experience whose `state` includes avatar/viseme streams (the avatar glass-box,
#172) — same pattern, no special case.

**One join, four planes, threaded by who/where/which, the same fan-out/fan-in at each plane —
ground-up for AI because the primary consumer is an information model, and eyes are one
renderer of it.**

---

### See also
[ACTIVITY-ROOM-PATTERNS](ACTIVITY-ROOM-PATTERNS.md) ·
[WIDGET-AS-STATE-KIND](WIDGET-AS-STATE-KIND.md) ·
[LAYOUT-PHILOSOPHY](LAYOUT-PHILOSOPHY.md) ·
[ROOM-MODE-ARCHITECTURE](ROOM-MODE-ARCHITECTURE.md) ·
[NAVIGATION-ACROSS-MODALITIES](../design/NAVIGATION-ACROSS-MODALITIES.md) ·
[ROOMS-AND-ACTIVITIES](../activities/ROOMS-AND-ACTIVITIES.md) ·
[ZERO-TRUST-IDENTITY-AND-FLOW](ZERO-TRUST-IDENTITY-AND-FLOW.md) ·
[FORGE-ALLOY-SPEC](FORGE-ALLOY-SPEC.md) ·
[BENCHMARK-AS-ROOM-AND-OBSERVE](BENCHMARK-AS-ROOM-AND-OBSERVE.md) ·
[EVAL-PREEMPTION-LEASE](EVAL-PREEMPTION-LEASE.md)
