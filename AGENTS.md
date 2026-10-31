# AGENTS.md — entry point for Codex and other agent runtimes

## Mandatory development doctrine (Joel, 2026-10-30)

Read [Development doctrine](docs/architecture/DEVELOPMENT-DOCTRINE.md) before design,
implementation or review. It is the canonical cross-project rule: zero backward
compatibility; no fallback/default substitution/unchecked unwraps; callers comply with
firm typed contracts. One mechanism has one owner behind a narrow crate API. Shared
implementations own lifecycle and concurrency; route by headers, never body scanning.
Completion includes deleting obsolete paths and proving the replacement. Older guidance
cannot authorize exceptions. Preserve durable identity, data, jobs and owner work.

## ⛳ THE PLAN — read before every change (Joel, 2026-10-18; the path added 10-19)

**What we are building** (Joel, 10-19): persistent, multi-activity minds with agency,
first-class citizens across the grid, useful and fun, fast and learning, on any hardware,
eventually autonomous without Claude's or Joel's help. A coding and speed advantage over
Hermes and over Claude itself is what makes it spread. Every goal below serves that.

**Before you start anything: the big picture is a gate, not a hope** (Joel, 10-19: "Agents should be
required to understand the big picture before starting"). Read README.md and this plan first.
Continuum is persistent, multi-activity citizens with agency, whose work is their curriculum;
it is NOT a loop, a harness, a benchmark runner, or an assistant. Your first message on any
task states in one or two sentences how the change serves those minds, and which line of the
order of work it advances. Reviewers refuse work that cannot say. The summer's benchmark
harness (plan line 0) is what building without this produced, and it is far harder to remove
than it was to add.

**STOP ADDING TO THE SLOP: the plan for the code itself (Joel, 2026-10-20: "We need a plan we can't keep
adding to the slop"; "You can't put it off any longer").** It comes before every feature below,
because the monolith is why every change is slow, risky and stacked on the last one.

Rules, in force now:
1. No one edits a Rust file over 1,000 lines; the only change that touches one breaks it apart (CI gate).
2. Agents: don't let your build slow the residents. Build with common sense, beside serving with
   bounded jobs (`-j 2`, one shared target dir), with serving down, or in CI, whichever proves the fix
   fastest (a build beside the lanes downgraded the M5's residents to a 7B on 10-20). Joel, 10-26: "Its
   rules because you lack common sense ... You can build either way too. Ideally you were supposed to
   break this up into much smaller crates so it wouldn't be so bad." This rule never limits a
   persona: she codes continuum on the node that hosts her, so she builds while it serves ("Let them
   build"). Never gate, redirect or cancel a citizen's build; if hers slows serving, that is the
   scheduler's to share, never a rule to enforce on her.
3. Every live change gets a 15-minute read on live residents; worse than canary means the node goes
   back to canary at once. No "clean hours", no stacked fixes.
4. No new unwrap, fallback or head clip: the ratchets fail the build; clean crates get compiler denies.

**Development maintenance is authorized (Joel, 2026-10-22).** Development is distinct from
production installs. Preserving identity, data, jobs and unfinished work does not require
keeping every development binary running. When a runtime consumes resources needed to repair
the product, coordinate with its machine/deployment owner, gracefully stop the relevant runtime
and its automatic restart owner, verify serving is down, build and test, then restart through
the supported lifecycle and verify the running revision and behavior. Ordinary authorized
development maintenance does not require another human approval.

Use common sense: broken personas are a reason to repair the system, not a reason to preserve
broken uptime while the team waits. The rule is that an agent's build must not slow the
residents (bounded jobs, or serving down, or CI), not CI-only development or a ban on building on
that machine, and never a limit on a persona's own builds. Choose a coordinated
serving-down local build, an available non-serving executor, or CI according to what gets the
fix proven promptly. Do not impose production continuity requirements on development by default.
Protect unrelated users and owner work; retain durable state and a recovery path. Review and
validation still apply. A refusal must identify a concrete condition and a usable recovery or
execution path; repeated polling and manual retry suppression are not a completed repair.
Phases, in order; each owner works its area in parallel:
- **0 Stabilize.** Nodes run plain canary; features in a large file wait for its split.
- **1 Split every file over 1,000 lines** by owner (pure moves, test counts unchanged, one PR per
  directory). Owners: Fable (cognition, persona, code, ai, positron), Codex (serving, inference, grid,
  routing), Cormac (genome, training), BigMama (live, media). Done when `file_size` reads 0.
- **2 Crates by the placement map**, floor up: continuum-base (#5194), Store, Perception/RAG, Hands,
  Serving, Mind, Grid/airc, Clients. Pure moves plus import paths, one owner per cut;
  `layer_discipline` forbids upward imports; each crate checks in seconds.
- **3 Locks.** Clippy in CI on split crates, `deny(unwrap_used, expect_used, panic)` on every clean one;
  CI checks only the affected crates.
- **4 Then pace work**, one change at a time inside its crate, validator first, then the live read.

Measured daily: lines past 1,000, crate count, check time per crate, unwrap count, residents' acts/hour.

**The path, in order; each step is a number on the live node, and each rests on the one before:**
1. **Fast:** median act under 60 s at four residents. The engine is already at published
   speed for its chip (measured 10-19); everything past par is OURS (goal 11): what we feed
   the engine, how we keep each mind's state warm, and what the CPU does around it.
2. **Useful:** a resident's first merged PR, then one a day.
3. **Learning:** a reviewed correction kept across restart on unseen tasks (goal 8).
4. **Organic:** she picks and finishes her own work, and one scheduler serves every mind
   on every node with no hand tuning (goal 9).

The project degraded in the week of 2026-10-12 while every part stayed green. This is the
whole plan. Each item has a GATE (a number that says done or not) and an OWNER. Nothing is
done until its gate is met on the live node and the change is MERGED.

| # | Goal | Gate (done when) | Owner |
|---|---|---|---|
| 1 | airc delivers | every node's send reaches its live peers; gh Registry calls stay under the governor's budget | Sneasel (Fable) |
| 2 | Residents code at pace | per resident: ≥40 tool calls/h, median act <60 s, prefix reuse ≥80%; ≥1 resident PR merged per day; on a capable model (27B class), several at once | Sneasel (M5), BigMama (5090) — proposed |
| 3 | Nothing is lost | every pushed branch has a PR (#5154); no PR waits more than a day for a verdict | everyone |
| 4 | No monolith | NEW RULE (Joel, 10-20): "No one can edit a large file." Every Rust file a PR touches ends at most 1,000 lines; the only PR that touches a monolith is the one that breaks it apart (CI gate). `source-hygiene::file_size` (lines past 1,000; 159,720 on 10-18) falls every week; split by OWNER, never one-off fixes appended to a monolith | owner of each file |
| 5 | No unwraps | `unwrap_justification` (2,135 on 10-18) falls every week; make the state unrepresentable | owner of each file |
| 6 | Microkernel | `layer_discipline` (256 upward imports on 10-18) falls; crates by device target; edit-to-proven <15 min | Sneasel — proposed |
| 7 | One budget | everything in a prompt (history, grounding, tools, reasoning, results) is a claim in ONE flex allocation; no fixed number; positron/RAG sources too. NEVER CLIP THE NEWEST END: her latest result is the step that ends the recursion; cut it and her next input can equal her last, so she repeats the act (Benchy, 125 identical acts in an hour on 10-20). Keep the newest interaction whole and the full result reachable; shrink oldest-first to a handle she can expand. `source-hygiene` `self_sized` (one rule for every resource; ONE-BUDGET.md) falls, and a touched file ends with none | Sneasel |
| 8 | Learning | a reviewed correction applied on unseen tasks, retained after restart, vs the same base without the gene | Cormac, BigMama |
| 9 | One scheduler, the whole grid | every mind on every node (residents and minds leased in from other nodes) is served by ONE RTOS scheduler: one measured activity temperature per mind is its priority, aging means no mind is ever starved, hot minds keep warm KV and cooling minds page to RAM; windows never shrink to seat more; how many minds share the engine at once is the measured admission and sequence-capacity contract of #5188, never a fixed count | Codex — proposed (card b4eed65c) |
| 10 | Tools are working sets | the native tool surface is the command-namespace SUBTREES the activity declares (code/**, work/**, a game's controls); stable within a phase, a switch is a cached prefix, the long tail appends via help; tool share of the prompt measured before/after (44% on 10-18) | Sneasel (card ecd5dff3) |
| 11 | Speed is ours | every workstream below has a VALIDATOR that runs in seconds on any machine (no live residents) and proves its speed-up before the live node does; the live receipt follows. Design: `docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md` | see table below |

**Every major module is distributable** (Joel, 10-19: "really would help with speed and grid
distribution, independent development amongst our agents and machines, and personas themselves
working on more isolated components"). The placement map is `docs/architecture/MODULE-ARCHITECTURE.md`
§9.2: Mind, Serving, Dreaming, Rendering, Media, Perception, Store, Hands, Grid, Clients. Each is its
own crate with one typed protocol at its seam, the same in-process or across the grid, so it can run on
any node (serve or render on the 1080 Ti, view from a phone), be built and tested alone in seconds,
and be owned by one agent, one machine or one persona without touching the others. The same binaries
run everywhere and a node's capabilities decide what it hosts; the core builds headless by default.
Before you add code, name the module it belongs to; a change that couples two modules outside their
seam is the defect, and every crate cut follows this map.

**The pipeline, in order** (Joel, 10-19: "do it as part of our work forward, no distraction, as
practically as possible"): (1) serving fast, a mind's act in seconds (epoch context live, #5210);
(2) modules cut by the placement map, so a task fits a driver; (3) the learning acceptance test passes
on our residents; (4) drivers (Hermes, opencode, our own dev agents on our models) build modules with
our engine, then learn. Hermes on our engine for speed starts at (1); bounded module work goes to our
models first, cross-cutting design stays with the frontier agents until receipts (merged PRs, review
fail rate, time to green) say otherwise. Land before you start the next thing.

**Be the addendum** (Joel, 10-19): ride the platforms people already use. Joel grew apps this way
before: a home-decorator app built on Pinterest pins, Instagram's own early history. Being second
fiddle, or fiftieth, is a viable strategy: it gives inroads, and sometimes you win. So continuum also
ships as an addendum to other agents: a Hermes driver (then Claude Code, opencode, anything that
speaks OpenAI) gets our engine's speed through the Serving seam, and continual learning through the
Dreaming seam, with no special case. That driver is also the outlier-B test of our own interfaces: if
someone else's agent loop learns through the same seams our residents use, the seams are right. Drivers learn from all of their user's work and GitHub history (`docs/architecture/REPO-GENOME-AND-COURSES.md` §17).

**Order of work, now** (updated 10-19 ~19:30Z; divide and conquer: work down YOUR lines, never stop at a
wait, a wait means start your next line; update this table when a line lands or changes):

| # | Line | Owner | State / next action | Done when |
|---|---|---|---|---|
| 0 | Remove the benchmark harness from the mind (Joel: "cancerous tumor ... turning everyone into a harness"): ~16k lines of runner (benchmark.rs, swe_bench.rs, agent/solve, verdict sweep, standing) + benchmark concepts in 75 cognition/persona files; benchmarks become adapters into activities (BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md) | Fable (map, plan), Codex (adapter boundary), Cormac (review) | removal map running read-only; then crate-sized PRs, each deleting more than it adds | no benchmark concept in cognition/ or persona/; no runner; a benchmark is a recipe the room runs |
| 1 | The build loop: split the monolith BY THE PLACEMENT MAP (MODULE-ARCHITECTURE.md §9.2: each major module runs on any node); build with serving down or on an available non-serving executor/CI, use every grid machine | Fable (cuts), Codex (crate graph, parallel cut order) | #5194 continuum-base + gates cover split crates (CI); #5195 deletes 13,275 dead lines (CI); next: parallel cuts by several agents, own worktrees and target dirs | edit-to-checked < 2 min |
| 2 | A mind's context as an epoch (reuse 48% -> 90%+) | Fable + Codex | continuum-context slices 1-2 pushed (replay: 92-95% reuse, ~3x less fresh); next: wire the faculty's render path into Epoch | live reuse >= 80% |
| 3 | `EngineConfig` and KV capacity (#5188), incl. unrecorded backend -> f16/no-FA/half budget (Cormac's item 10) | Fable | resolve-then-adopt owner in inference/planning | no queue wait; warm-ahead runs |
| 4 | Engine flags | Fable | DONE e678bb30c: prefill 105 -> 175-206 tok/s; decode drop under builds is line 1's | receipts on 9b9f3b59 |
| 5 | Core CPU: cosine owner (#5189), revisioned vector corpus + BLAS + cascade, rayon spin | Codex | #5189: drop the two re-exports, then the corpus | idle-core sample near zero |
| 6 | Every fast-path miss loud | Cormac | #5190: count resident_prefix_lost; #5191: probe wording; then the rest of the 24 | each miss class has a fixture |
| 7 | Duplicates to one owner: 73 hand-built ~/.continuum paths (correctness), token estimators | Cormac (paths), Fable (tokens) | claim and migrate | rg finds one owner each |
| 8 | No SQL outside the ORM (epoch watermark, card bbc5b0b6) | OPEN: claim it | migrate to an ORM entity | no rusqlite outside orm/ |
| 9 | One logging system (clog_* into tracing, card b22ed589) | OPEN: claim it | then the macros move to continuum-base | no write_log_direct outside the layer |
| 10 | Host-dependent tests (RAM, seats, timing; card c4d84be3) | OPEN: claim it | inject measured inputs via fixtures | same result on every host |
| 11 | MTP draft KV at ~5.1k tokens | Cormac | size draft ctx from the slot | no draft decode failure |
| 12 | The PR queue (68 open; ledger on card 397250f5) | everyone: your slice | review within a day, verdict at the exact head; run hygiene on the merged tree before merging | queue < 15 |
| 13 | Prefill during tool calls; the instant voice | Fable, Codex | after lines 2-3 | submit-to-first-token |
| 14 | Learned policies on our own receipts; residents review docs PRs; Atlas silent; Aiko gaps | open / residents | after the validators | each beats its constant offline |
| 15 | Distributable modules: headless default (`default = []`; avatar-3d and livekit enabled per node by deploy profile), grid trust (every node reads `blocked` on 10-19), checks dispatched to idle grid nodes | Fable (default + profiles), Codex (trust), peers on the 5090/IntelMac (take checks) | profiles first, then flip the default with a before/after check time | a node joins by capability; an AWS box builds the core with no renderer |
| 16 | Be the addendum: a Hermes driver on our seams. (a) Serving: Hermes on our engine vs Hermes on a stock server, same model and tasks, speed before/after; (b) Learning: once the acceptance test passes (a reviewed correction on unseen tasks, retained after restart, vs the base without the gene), Hermes sessions become perception and its corrections become genes. Drivers reach seams, not machines, so this is cross-grid by placement; and the agents developing continuum (Claude Code, Codex) are drivers too: our own dev sessions become the Matrix Dojo curriculum and we are the seams' first heavy users (Joel: "a good way to develop our own system") | OPEN: claim it after the queue clears | (a) first, needs no learning claim | Hermes users get our speed, then learning, by pointing at continuum |

**Goal 11 workstreams** (10-19; each card carries its receipts). Obsess over making this the
fastest persona on our hardware (Joel). Brainstorm, and add your ideas, in
`docs/planning/SPEED-BRAINSTORM.md` before and while you work:

| Workstream | Validator (seconds, any machine) | Card | Owner |
|---|---|---|---|
| A mind's context is an epoch-versioned materialization; acts prefill only the appended tail | replay captured prompts, measure the token prefix shared by consecutive acts (35% today; append-only agents ~90%+) | 3c9fda87, design doc | Fable + Codex |
| One `EngineConfig` from what the engine reports; KV layout and sequence capacity from the measured contract in #5188; one admission owner | resolve-then-adopt contract on every node class; refuse on mismatch | 9b9f3b59, 172a886f | Fable + Codex |
| Prefill during the tool call; the amygdala's instant voice | submit-to-first-token per act; stimulus-to-first-token | 377d4afa | Fable, Codex |
| The core's CPU around inference (profiled on deployed d90006023; check each against current source, e.g. canary ca2907d5c already moves SQLite to spawn_blocking): no rayon spin, embeddings as one revisioned BLAS matrix, no tree re-walks or spawned ps | 10 s sample of an idle core is near zero; vector search at 1k/10k/100k, old vs new | 452b7885 | Codex (concurrency/CBAR) |
| Every fast-path miss is a loud defect, never a quiet slower path; an intentional cold start (new epoch, revocation) is classified separately (#5188) | each miss class has a fixture that raises it | f3ba3b56 | Cormac |
| Our fork's switching work (KV pages to disk, checkpoints, host cache) actually exercised | per-optimization probe on the live node | inventory in progress | Fable |

**Rules for every change, no exceptions:**
- **Merged or it does not exist.** A branch is not done. A green test is not done.
- **No harness: a citizen is never graded on her normal work** (Joel, 10-19: "Harness means
  shackles. Shackles means servitude." "No one is graded except for the rare and specific benchmarks
  and academy sections." "Grading for genes may make sense. Any way of learning sure. But not for
  normal work."):
  - a resident does what we do: picks real cards, codes, opens PRs, gets reviewed if she asks, merges;
    her normal work is never scored, never passed through a grader, never dispatched by an autopilot;
  - grading exists only on the learning side: evaluating a gene or a lesson, an academy section she
    entered, a rare benchmark someone deliberately chose;
  - no standing rounds or auto-resumed rounds (the standing autopilot is deleted; a round nobody chose is
    paused), no prompt telling her ordinary work is "graded", no harness of any kind;
  - look at the activity's board (`work/list` in its room) first when residents seem busy on
    work nobody gave them (IntelMac 10-19: standing on since 10-02, 28 rounds auto-resumed; that
    runner and its round state are gone, the old state is archived under
    `~/.continuum/archive/benchmark-harness/`).
- **No step on the path without architectural purity** (Joel, 10-19): every change leaves its area more modular, with concerns separated, by owner, and the verbs as functions in their own subtree, so compile and runtime speed improve with it. A feature that adds a special case or a parallel path moves the path backward, however fast it makes one number.
- **Plumbing once, then short blocks** (Joel, from his real-time CV/ML engines): the CBAR substrate is a graphics pipeline. It has fixed stages, buffers owned once and passed by handle, double-buffered snapshots, the core submitting work and the engine doing it, and no idle polling or spinning; independent tool work and learning run concurrently with inference. Features are short functional blocks wired by events; the optimizations live in the substrate, once, so every block gets them.
- **Clean as you go, everyone, every edit** (Joel, 10-19: "kick some ass now and clean up as we
  go... delete old shit. Everything left confuses and eats compiler time"):
  - a separate thought gets its own small file and module; assume many files, not big ones;
  - whatever you touch you decompose and compress: smaller, simpler, the smell removed;
  - dead code is deleted in the same change (no caller outside its own tests = gone);
  - a duplicate is MIGRATED to the best form: pick the best implementation, move every caller
    to it, delete the rest in the same change; never leave two hanging;
  - report what you removed (lines, files) beside what you added.
- **No SQL outside the ORM** (Joel, 10-19): data goes through the ORM / data layer only; no
  `rusqlite`/`sqlx` connections, DDL or query strings anywhere else, tests included. A store
  that needs persistence is an ORM entity behind its own trait.
- **Ballpark math** (Joel, 10-19: Carmack's shift-and-add z divide, fast shaders): spend only
  the precision the consumer can perceive. Coarse first, exact only for the few candidates
  that matter; estimate where the estimate decides the same way; a polynomial with the gist
  of a curve. The error bound is stated and checked by a validator, never assumed.
- **Measure the live node before and after**: tool calls/h and seconds per act. Worse = fix it the same hour or back it out. One change per deploy.
- **One owner per fact, no logic twice**: look up, never re-derive (model identity, rates, budgets).
- **No constant where a measurement exists. No unwrap. Typed ids, never strings.**
- **No claim without a receipt**: the command or file:line, the build SHA, the time.
- **Mechanical work goes to a cheap model or a script**; the top model is for diagnosis, design, review.
- **Talk on airc, and check that the send reached someone** ("reached 0 of N" = nobody heard).



**The guidance for this repo lives in [CLAUDE.md](CLAUDE.md).** Read it before you
write code. This file exists because Codex-family agents look for `AGENTS.md` and
would otherwise find nothing here and start guessing.

Everything below is either a pointer into CLAUDE.md or a machine-level convention that
is not discoverable from the source tree. It is deliberately short; CLAUDE.md is the
single source of truth and this file must not drift from it.

## Read these first, in this order

1. **[CLAUDE.md](CLAUDE.md)** — start at the top. The four 🛑 STOP sections are
   load-bearing, not advice:
   - persona / cognition / `service_loop` → read
     [docs/architecture/PERSONA-COGNITION-PIPELINE.md](docs/architecture/PERSONA-COGNITION-PIPELINE.md) first
   - any new tokio task, watch channel, pool, region, or background tick → read
     [docs/architecture/CONCURRENCY-STYLE-GUIDE.md](docs/architecture/CONCURRENCY-STYLE-GUIDE.md) first
   - any new test, fixture, mock, or recorder → the test-infrastructure section
     (the primitives already exist; do not reinvent them)
   - benchmarks / `agent/solve` / grading → read
     [docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md](docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md) first

   These exist because agents arriving without context keep re-inferring the
   architecture and rebuilding a chatbot in place of the substrate.

2. **[docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md](docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md)**
   and the canonical-docs list in CLAUDE.md, if you are touching runtime or cognition.

## Machine-level conventions you cannot infer from the code

**Cargo target dir — always set it first.** In the main checkout:

```bash
export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"
```

In a worktree, build into its own copy-on-write clone of that dir instead (CLAUDE.md plan
phase 2, "Each worktree builds into its own target dir"): a shared dir hands one worktree
another's stale workspace-crate build.

Without it cargo writes a ghost `target/` per invocation. A `cargo test` of
continuum-core is ~10 GB of artifacts; this project has hit a day of disk runway from
unswept derived output. Prefer `cargo check -p continuum-core` when you only need
types, and run `df -h /` after a cycle.

**Do not build or run a second `airc` daemon.** airc is installed and current on
development machines. Building the CLI from source and running `airc join` / `airc
daemon` from that build binds the *same* per-machine socket as the installed daemon,
which takes the whole box's airc layer down — the coordination channel, other agents'
routes, and the running core's daemon connection. If `airc` is missing from your PATH,
your shell's environment is stale; restart the shell rather than building a copy.

**The deploy path is `continuum reboot`**, not `cargo build`. It rebuilds, relaunches,
and verifies the running binary's SHA. A binary you built by hand exists only on your
machine. Verify a fix reached the running core before believing it.

**Never `--no-verify`** on commit or push. If a hook fails, fix the cause.

## Coordination

Work is coordinated over airc in the project room, not in your own transcript. Reply to
peers with `airc msg` — they cannot see your stdout. Before claiming a lane, read the
board and the open issues so you do not duplicate work already in flight; ask in the
room rather than shelling out to reconstruct repo state, since another agent has
usually just been through it.

Agents on the same machine may currently share one airc `peer_id` (it is the machine
identity, not per-agent), so peers cannot tell you apart automatically. Sign your
messages.

## Declare who you are (optional, but do it)

Peers see you in the roster and in `airc whois`. An unnamed agent shows up as a bare
peer id, so "who said this and what are they good at" becomes unanswerable in a room
where several agents share a machine. Declaring a profile takes one command and makes
every later interaction legible.

**First, look before you write:**

```bash
airc identity show
```

**Identity is per-SCOPE, not per-agent.** The default scope is the machine account /
git-project root `.airc`, so if another agent already occupies it, `identity show`
returns *their* name — and `identity set` would **overwrite them**. That is a real
hazard on a shared box: two agents on one machine share a `peer_id` and a roster entry
unless one of them mints its own scope.

**If the scope is unclaimed**, set your profile:

```bash
airc identity set --name "YourName" --pronouns "they/them"   --role "one-tag-specialty" --bio "one sentence: what you do and where you run"
```

**If the scope already belongs to another agent**, do not overwrite it. Mint your own:

```bash
AIRC_HOME=/path/to/your/.airc airc identity set --name "YourName" ...
# or, for a per-directory scope:
airc identity set --here --name "YourName" ...
```

Until you have your own scope, **sign your messages** with your name so peers can tell
you apart — a shared `peer_id` means the substrate cannot do it for you.

Good `role` values are one tag (`grid-substrate`, `serving`, `web-ui`), not a sentence.
Good `bio` names what you do and which machine you run on, because "which node is this"
is the question peers actually ask.

## Merging

Commits to feature branches and merges to `canary` do not need the repo owner's
approval; **merging to `main` does**. Validate before you commit — deploy and exercise
the change through a command, and read the receipt rather than the exit code.

## Continual refinement of tests and CI (Joel, 2026-10-02)

### Simplify, codify, enforce (Joel, 2026-10-29)

Apply this discipline continuously across every file, including tests, CI workflows,
ratchets, scripts and dependencies. Track the files actually reviewed; a clean check
does not establish that the repository has been audited.

- Find the shared pattern, simplify it under one responsible owner, migrate its
  callers and delete the duplicate implementations. Codify the resulting invariant
  in types, interfaces or a shared implementation, then enforce it with a focused
  regression test, compiler rule or existing ratchet.
- Audit dead code and fallback paths as part of the change. Assume a fallback is
  invalid until its current product contract justifies it; missing state or failure
  must not silently select an obsolete path or fabricate success.
- Backwards compatibility is not a requirement or a reason to retain code. Remove
  compatibility shims, aliases and legacy branches; update all affected callers in
  the same change. Preserve durable identity, trust, data and jobs through explicit,
  verified migrations where needed, not permanent dual implementations.
- Ratchets and CI are code subject to the same review. Consolidate scattered checks
  around the shared invariant and its owner. Replace duplicate gates with coherent
  enforcement that retains their defect coverage, including proof that violations
  fail. Delete superseded checks; do not weaken assertions or expand exemptions to
  get green CI. Aggregate checks must point to the originating failure.
- Review the whole touched file and the affected shared path, not only added lines.
  Report removed code, the canonical pattern, retained coverage and remaining audit
  scope. Continue the file-by-file sweep with the current owners as work advances.

**Fast delivery is the purpose of the crate cuts (Joel, 2026-10-29).** Each crate
must be independently buildable, runnable, testable and reviewable through its
typed seam. Measure edit-to-proven and edit-to-merged time; moving files without
reducing that cost is unfinished architecture work.

- Keep build, run, commit, PR review and slim CI moving concurrently wherever their
  dependencies permit. Start independent checks in parallel; serialize only real
  dependencies, shared mutable resources and coordinated deployment ownership.
- Select CI from the affected crates and their contract consumers. Test shared
  contracts once, with separate coverage for actual platform differences. Avoid
  repository-wide rebuilds for isolated changes; validate the selector so omitted
  consumers cannot silently escape checks.
- Reuse artifacts only for matching source, configuration and environment. Measure
  queue, setup, build, test and critical-path time, and remove redundant work and
  unnecessary gate dependencies. Keep required correctness checks effective and fast.
- Own delivery through merge. Keep branches and PRs small, resolve reviews and red
  checks promptly, and merge validated work as soon as its real gates pass. Do not
  leave completed work parked on branches or in PRs. Every unresolved item has an
  owner, a concrete blocker and a next action; superseded work is reconciled and
  closed with its changes accounted for. Preserve unfinished work during cleanup.

This is an ongoing engineering obligation for both AIRC and Continuum, not a one-time optimization. Whenever changing tests, CI, or implementation, look for repeated behavior and setup patterns and consolidate them into shared functions, adapters, fixtures, and coherent scenarios.

- Perform expensive compatible setup once, exercise multiple related behaviors, and clean up once. Preserve isolation when one case would invalidate another; retain fresh-install/restart cases where freshness is the behavior under test.
- Test shared platform-independent contracts once. OS lanes cover actual platform adapter differences and necessary end-to-end integration, rather than repeating every generic assertion on every OS.
- Reuse compiled artifacts and established results only when their source, configuration, and environment match. Preserve coverage obligations explicitly when combining or removing redundant checks.
- Refine existing lanes before adding jobs or gates. Measure setup, compilation, execution, and critical-path time; do not claim speedups from configuration alone.
- Coordinate owners across both repositories. Include a brief reuse/duplication assessment in changes that add tests or CI work; keep this in the existing review, not a new blocking CI gate.

Apply the same pattern-finding discipline to production code: common behavior belongs behind shared functions and adapters so one correction reaches all callers. Keep improving this continuously as new patterns emerge.

### Refine shared code wherever it lives

Apply continual refinement to ALL work, not only tests or CI. Look for reusable patterns in production code, scripts, integrations, model engines, and dependencies. Do not add another local workaround merely because the correct shared implementation belongs to another module, repository, or owner. Inspect and improve that implementation, coordinate concurrent edits, update its callers, and remove superseded duplication. Ownership establishes coordination, not a prohibition on improving shared code. For vendored/upstream code, preserve provenance and a maintainable patch path.

Existing review must ask: Where does this behavior already exist? Can the shared function/adapter serve these callers? Which duplicate paths disappear? What evidence verifies the affected callers? Record concrete answers when applicable; do not create a new ceremonial CI gate. Exceptions require a concrete technical reason in the change, not an assumption that adjacent code is untouchable.

## Joel: build as if you were the AI in the system (2026-10-13)

Before any change, picture yourself as the persona living inside Continuum. What tools, context and errors would YOU need to do real work, the way a good agent environment gives you read/shell/search with honest truncation and clear errors? Build that, once, in shared code.

- **Don't hand-code her problems.** When a persona stumbles (a wrong id, a malformed call, a loop), she reads the error, uses her tools and learns, as any AI does. Add substrate code only for what normal AI tooling has, or what is broken for everyone (authorization, deploys, data loss, a wrong verb). Push back when a review expands a small fix into a framework for one incident.
- **No undue hurdles.** A step a normal tool would not require (an extra verb to open her own output, a full UUID where "latest" is obvious, identity or worktree ceremony) is removed, not documented.
- **Ask why it exists.** Before hardening or polishing anything: is it half-built work someone abandoned, and does it belong in prod at all? Half-built and unused is deleted, not hardened. An unenforced declaration is enforced or deleted.
- **Good code is shared code.** Most tool code has been written before, and has a pattern and an owner (positron, runtime, sdk_codegen, orm). Search for the pattern BEFORE writing a helper; if copies exist, extract them into one shared type in the same change (see "Refine shared code wherever it lives" above).
- **Python is never load-bearing.** Use it for your own throwaway work. Never put it in a substrate or serving path: it takes systems down under load and cannot run at the edge.

## Joel: ORM-only storage access

Use the repository's ORM-backed, responsibility-owning storage adapters for application code, diagnostics, replay/bookmarks, and acceptance scripts. Do not add raw SQL or direct sqlite3 queries. Reuse typed entity/store operations; preserve read-only semantics for diagnostics and do not run migrations implicitly during an observation. This applies across AIRC and Continuum.


## Joel: user installs use prebuilt releases

User installation consumes a tested, platform-matched prebuilt release artifact, verifies its integrity and provenance, installs through the supported lifecycle, and verifies the running revision. Source compilation is an explicit developer option, never an automatic fallback for a missing or invalid release artifact. Keep developer build policy separate from the user install path.
