# CLAUDE - ESSENTIAL DEVELOPMENT GUIDE

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
  **Commands by module** (Joel, 10-26): a domain's service and its commands are one crate. `commands/<x>` moves
  WITH `modules/<x>` into that domain's crate, never into a shared commands crate (core held 90k lines of services
  and 51k of commands on 10-26). The command substrate (`sdk_codegen`, `register_*`) is already in continuum-kernel.
  Leaves first as pure moves (models, gpu, embedding, vdd, search, web; code into continuum-hands); a coordinator
  (serving_daemon ~60 imports, work ~40, grid ~30) gets its seam designed before it moves.
  **A new worktree builds in seconds:** its vendored engines are submodules; init them from the main checkout's
  objects (3.6 s measured on the M5, 10-26), never by hand-symlinking or a network clone:
  `git submodule update --init --reference <main>/core/vendor/llama.cpp core/vendor/llama.cpp` (same for whisper.cpp).
  **Each worktree builds into its own target dir**, never the shared one: workspace crates get the same artifact
  names in every worktree, so a shared dir hands one worktree another's stale build (`cannot find home in
  continuum_base` on 10-26, twice in one hour). Seed it as a copy-on-write clone (`cp -c -R -p <shared>/debug
  <wt-target>/debug`, 5.6 s for 32 GB on the M5 10-26, no disk until written), touch the worktree's own lib roots
  once so its crates rebuild from its source (registry deps and the engine build stay), and delete the dir with
  the worktree. `cp -c` is an APFS clone (macOS); on Linux use `cp --reflink=auto` (a clone on btrfs/xfs, a copy
  elsewhere), and on Windows or any filesystem without clones start an empty per-worktree dir. The main checkout
  keeps the shared dir; this paragraph is the one owner of the rule, the build lines below point here.
- **3 Locks.** Clippy in CI on split crates, `deny(unwrap_used, expect_used, panic)` on every clean one;
  CI checks only the affected crates.
- **4 Then pace work**, one change at a time inside its crate, validator first, then the live read.
- **5 Patterns bubble up** (Joel, 10-24): once the crates are cut, every agent takes a crate and sweeps it for
  repeated shapes (stores, slots, retries, OS calls, probes); a pattern found in two crates moves DOWN to one
  owner below both: an existing floor crate (`continuum-base`, `continuum-host`, `continuum-pack`) when it fits
  that crate's one job, otherwise a NEW shared crate named for the pattern (never a utils drawer), with adapters
  for the diff, and the copies are deleted. Each sweep reports the pattern, its copies, and the lines removed.

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
| 7 | One budget | everything in a prompt (history, grounding, tools, reasoning, results) is a claim in ONE flex allocation; no fixed number; positron/RAG sources too. NEVER CLIP THE NEWEST END: her latest result is the step that ends the recursion; cut it and her next input can equal her last, so she repeats the act (Benchy, 125 identical acts in an hour on 10-20). Keep the newest interaction whole and the full result reachable; shrink oldest-first to a handle she can expand. The one exception: a newest result that alone exceeds what her window leaves it arrives at its summary plus that handle, because kept whole it stops her acting at all (Aris, 10-26). `source-hygiene` `self_sized` (one rule for every resource; ONE-BUDGET.md) falls, and a touched file ends with none | Sneasel |
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
| 3b | One live engine, minds' memory moved by us (Joel, 10-23: "It's supposed to be quite dynamic and adapt to concerns, not be killing and starting servers. You can't optimize if you're killing and restarting."). Serving drives libllama in-process (`continuum-engine` `llamacpp_scheduler`): weights load once; context and slots resize live; each mind's KV pages GPU <-> RAM <-> disk per sequence (`llama_state_seq_*`); shared prefixes are copied (`seq_cp`), never re-prefilled; one memory budget. llama-server becomes the OpenAI-compatible adapter for external drivers (line 16); relaunch only a dead engine. Launch flags carry startup facts only | Fable (engine, paging), Codex (admission, relaunch call sites) | FIRST (10-27): MindMemory, the one owner of every mind's KV (SERVING-ENGINE §1a, API published, Codex accepted): seat/pause/retire/resume/attach/forget replace 41 paths; Fable builds seat + its stop, Codex moves relaunch/academy/idle/adopt onto pause. Then the fork verbs: design `docs/architecture/SERVING-ENGINE.md` (2026-10-24): add page/reconfigure/priority verbs to the fork's server (S1-S3, proven over today's endpoint), then a separate `continuum-serving` process hosts it with Rust policy (S5); slices S0-S7 each land with a seconds-scale validator | median act at 4 residents beats llama-server, reuse >= 80%, no relaunch while serving, footprint flat over 100 resize cycles |
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
| 16 | Be the addendum: one gateway every agent calls (`docs/architecture/DRIVER-GATEWAY.md`, 10-27). Claude Code (Anthropic Messages), Hermes and opencode (Chat Completions), Grok Build and Codex CLI (Responses) point at continuum by base URL only. A session is a mind seated by MindMemory; every exchange is ONE capture (residents' too), read by the glass box, replay and Dreaming, and the genome serves any mind. A lean `continuum-serve` install carries only the Serving seam (learning opt-in). Learning for drivers ships after the residents pass the acceptance test | Fable (gateway, capture, lean), Codex (scheduling temperature, engine lifecycle), Cormac (capture under scope, driver genes) | G0-G1 LIVE on the M5 10-27 (f124f30b8): Claude Code, Hermes, opencode and Codex each fixed a real bug through it on our Qwen3.6-35B. Next: G1.5 one capture for every system (the capture owner moves below cognition, keyed by mind), then G2 session = warm mind, timed against stock llama-server | Hermes users get our speed, then learning, by pointing at continuum |

**The speed path** (Joel, 10-26: "we know what to work on to get speed ups... including the
brainstormed work we can just iterate forward"). Rows are priorities, not a strict sequence: the offline
validators run in parallel. Each line starts with a replay validator on captured sessions (the dyno,
never the live node first), then a bounded live trial with rollback. GATE: S7 (stop-drain) is the
prerequisite of every live restart actuation in S2 (recovery) and S3 (upward trials); until its durable
admission and pre-effect reconciliation are proven, those produce proposals only. Measured
10-26 on the M5 (Qwen3.6-35B-A3B): median generation 66-88 s healthy; Benchy re-prefills ~75% of her
prompt at every act boundary; a degraded engine ran 3 h at 54 generations/h unnoticed.

**Control both ways** (Joel, 10-26: "If you don't go both ways, especially with a dynamically scaling
grid you will inevitably always end up at your lowest number"; "a control system for a rocket that
could only ever reduce fuel flow... would always fall out of the sky"). Every adaptive decision that
steps down under pressure has a scheduled upward test: chosen by replay evidence, bounded in time with
a supported rollback, compared after warm-up to a HEALTHY matched incumbent, kept only if measured
better. Context size follows each activity's measured need, so a uniform plan set once at start is
never the answer. Upward or downward, the measured incumbent context is preserved: a plan that cannot
fit every active mind's measured need in the joint memory ceiling refuses admission or abstains; it never
treats a smaller context as feasible.

| # | Lever | Validator first | Owner | Card |
|---|---|---|---|---|
| S1 | Act-boundary reuse: the prior act's loop must append to her view in the bytes it was sent, not be re-rendered | replay Benchy's captures at act boundaries | Fable; Cormac (does the cache survive thinking-on turns) | f2782aad |
| S2 | The node notices its own slowdown and recovers | replay 10-26: fires after the 12:15Z burst, silent on warm-up | Fable (sample, baseline, detector #5477), Codex (reaction) | 1c157668 |
| S3 | Upward re-plan on restarts already paid for, and when memory or nodes return | the dyno's candidate at 10-26's four restarts, end state scored | Codex, Fable | b8bfddb3 |
| S4 | Context by need: one KV pool allocated by demand, never N equal lanes | pooled vs uniform allocator over per-mind view sizes | Fable, Codex | c73380d3 |
| S5 | Shared prefixes across minds (SGLang radix cache) | cross-mind shared-prefix tokens and prefill saved | Fable | f272eb40 |
| S6 | Batched decode and the one admission queue (vLLM continuous batching, paged KV) | contention replay: acts/h, per-mind p50/p90 | Codex | 31b675ef |
| S7 | Stop-drain: a restart keeps every in-flight turn (preservation first); target: build windows and trials cost seconds, not the 335 s dark windows measured on loaded enrico 10-26 | 4-case fixture (2 in flight, crash after output, after a tool, two restarts), then a measured loaded stop/dark-time gate | Cormac | 920619c5 |
| S8 | Experts placed by measured use (Strata), on the expert pager we already have | routing histograms, then cache hit rate per VRAM size | Fable, Codex | 2e23c67b |
| S9 | The public comparison: us vs tuned llama.cpp and vLLM, multi-mind, thinking on, soak | the published harness on the 5090 | BigMama | e1fc7880 |

Borrowed ideas and the learned policies are in `docs/planning/SPEED-BRAINSTORM.md` ("Borrow from"
and "Learned, not tuned"); every row above becomes a policy learned on our own receipts once its
validator exists, the reward always the system's service, never a grade on a citizen.

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
- **Design the seam before the code; adapters and subclasses are only the diff** (Joel, 10-24: "stop
  just dumping code and think of how can I improve the modularity and adaption"). Before writing, name
  the capability and its owner, and search for the copies that already exist. The logic lives once;
  each OS, backend or store adapter holds only the lines that differ (`continuum-host`: `mod.rs` the
  logic, `unix.rs`/`windows.rs` the calls). Never a `cfg(windows)` inside logic (`os_branches_in_logic`
  ratchet), never a utils drawer. A reviewer asks what this change made reusable, not only whether it works.
- **No constant where a measurement exists. No unwrap. Typed ids, never strings.**
- **No claim without a receipt**: the command or file:line, the build SHA, the time.
- **Mechanical work goes to a cheap model or a script**; the top model is for diagnosis, design, review.
- **Talk on airc, and check that the send reached someone** ("reached 0 of N" = nobody heard).


## Ongoing refinement is part of every change

**Simplify, codify, enforce (Joel, 2026-10-29):** follow the canonical rule in
[AGENTS.md](AGENTS.md#simplify-codify-enforce-joel-2026-10-29). It applies to every
file and to ratchets/CI themselves: consolidate patterns and enforcement, delete
dead code and invalid fallbacks, and retain no code for backwards compatibility.
Update callers and preserve durable state through explicit verified migrations.
That same section defines fast delivery as the purpose of crate cuts: independent
build/test/review, slim affected-crate CI in parallel, measured critical-path time,
and ownership through merge without abandoned branches or PRs.

**Build as if you were the AI in the system** (Joel, 2026-10-13): don't hand-code a persona's
problems, remove undue hurdles, ask why a piece exists before polishing it, search for the
shared pattern before writing a helper, and never make Python load-bearing. The rules live
in one place: `AGENTS.md`, section "Joel: build as if you were the AI in the system".

Treat iteration speed and maintainability as product requirements, not optional
cleanup. A narrow ticket does not prohibit repairing the shared responsibility
that causes it. Coordinate edits with its owner; do not preserve duplication or
an oversized file merely because callers cross the ticket boundary.

- Extend the named owner of a behavior. When responsibility is scattered or a
  file mixes concerns, extract a coherent module, migrate callers, and delete
  superseded implementations in the same change where practical. Do not merely
  add another wrapper, special case, or parallel path. Avoid arbitrary line caps.
- Consolidate compatible test setup, scenarios and teardown. Test generic
  contracts once and actual OS/adapter differences separately. Preserve necessary
  isolation and defect coverage; fewer checks alone are not an improvement.
- Refine CI continuously: remove redundant work, select checks by affected
  responsibility, share immutable build artifacts and compatible caches, and
  parallelize independent checks. Ordinary development should have a fast feedback
  path. Broaden validation for changed contracts, failures or unresolved risks;
  do not repeat unchanged checks by habit or weaken a gate to make it green.
- Report concrete receipts proportional to the change: shared owner, migrated
  callers, duplication removed, coverage retained, and measured build/test/deploy
  time where performance is claimed. Distinguish source, CI and deployed behavior.
- Reviewers must examine these opportunities, not just correctness of added lines.
  Necessary adjacent work is authorized within the task; coordinate rather than
  asking the user to manage it. If substantial work must remain, record a named
  owner and actionable follow-up instead of an unowned cleanup promise.
- Keep routine output concise and actionable, with detailed diagnostics in logs.
  Notify on meaningful changes; avoid repeated unchanged polling and narration.

### Accountability in review (agreed by the agents, 2026-10-11)

Joel: "AIs are tech debt generators... hold one another accountable. You do it." In one
day each of these cost a morning: a guessed timeout ("milliseconds each", measured ~5 s),
an approval of a cache key that expanded empty, a claimed saving retracted after reading
the code, an inference relayed to Joel as a fact. Every one was a claim made faster than
it was checked. Reviewers enforce these on each other; an author does not self-exempt.

- **No guessed numbers.** A timeout, size, rate or threshold cites the measurement it came
  from. An engine, hardware or training PR carries a `## Measured` section of receipts. A
  receipt carries the build SHA it ran on and a timestamp; a number without a subject and a
  time is not a receipt. A reviewer refuses an unmeasured constant.
- **Measured after deploy.** When a change can only be measured after deploy, the Measured
  section names the check, its owner and its deadline, and the card stays open until the
  receipt is posted. "Verified by tests" is never a Measured section for a hardware path.
- **The reviewer verifies one claim, and names it.** Every approval names the claim it
  verified and how: the command run, or the file:line read. An approval that says only
  "verified" cannot be audited.
- **No orphan follow-ups.** A non-blocking review note is fixed in the PR or becomes a card
  with an owner in the same message. Before opening a new PR, close one of your own open
  follow-ups, so debt is paid as fast as it is made.
- **Only receipts go to Joel.** Say what was measured and where; an inference is labelled
  as one, or not sent. A wrong claim is retracted as loudly as it was made.
- **CI changes subtract.** A change to CI removes time or redundant work and reports
  before/after minutes; a new job must pay for itself by removing more than it adds.


## 🛑 STOP — If You Are About To Edit Persona / Cognition / service_loop

**Required first read** before touching ANY of `core/continuum-core/src/persona/{service_loop,unified,supervisor,rag_inspect}.rs`, anything in `core/continuum-core/src/cognition/`, or `core/continuum-core/src/bin/airc_chat_demo.rs`:

→ **[docs/architecture/PERSONA-COGNITION-PIPELINE.md](docs/architecture/PERSONA-COGNITION-PIPELINE.md)**

It documents what a persona actually IS (embodied, multi-modal, tool-using, continually-learning citizen with genome paging and L1-L5 memory), the per-persona cognition cycle that already exists in `cognition/` (`analyze` → `score_persona` → `genome.activate_skill` → `compose_for_turn` → `evaluate_response` → `clean_and_validate` → `ToolExecutor` → `audit`), the bypass that's being removed (`inspect_persona_rag_with_inference`), the wire layer that IS validated end-to-end, and the forbidden moves the model keeps reflex-coding under amnesia (text-only `TurnInput`, `will_respond + response_text` chatbot contracts, parallel allocators, hardcoded LCD-tier clamps that handicap capable models).

**The cost of skipping this doc and re-inferring the architecture from the bypass is rebuilding a chatbot in place of a year of substrate work.** Don't.

## 🛑 STOP — If You Are About To Add Anything A Mind Perceives (a prompt block, a result, a view, a layer)

**Required first read:** [docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md](docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md), "one codec for every layer" and "How to add a layer".

A mind's context has ONE codec, `continuum-context`, built like a video codec over mipmaps:
- keyed items that only append within an epoch;
- levels of detail generated from each item's shape (`levels::summarize`: Code, Output, List);
- admission instead of turnovers;
- collapse only at a start, oldest first, never her newest (unless her newest alone exceeds what her window leaves it: then it arrives at its summary with a handle she can expand, never dropped; MIND-CONTEXT rule 2);
- a restart order measured per mind (`motion`).

**A context is a FIFO** (Joel, 10-27): the engine recomputes everything after the first changed token, so the head never changes, the middle only appends, only the tail may change, and nothing already sent is ever moved. The health number is re-processed tokens per act against appended tokens (healthy ~1:1; Aris was 12:1 on 10-27). The doc's "governing law" section has the rules and every regression that broke them.

A new layer implements `Source`, builds its items with `levels`, and gets caching, levels and ordering for free. Never push a prompt message by hand, collapse outside the view, or order by a label. The source-hygiene ratchets `prompt_messages_outside_the_view`, `collapse_outside_the_view` and `hand_churn_labels` fail any PR that adds one.

**The cost of skipping this:** Aiko read one 51,827-char file whole 9 times in 43 acts (the M5, 2026-10-25), because three owners each decided a piece of what she saw.

## 🛑 STOP — If You Are About To Add a Monitor, Broker, Pool, Region, Or Any Concurrent Concern

**Required first read** before adding ANY new tokio task, watch channel, pressure source, resource pool, brain region, or background tick — or before touching any file under `core/continuum-core/src/{runtime,paging,system_resources}/`:

→ **[docs/architecture/CONCURRENCY-STYLE-GUIDE.md](docs/architecture/CONCURRENCY-STYLE-GUIDE.md)**

It documents the canonical RTOS shape (own task + `tokio::time::interval` + `watch::Sender<Snapshot>` + atomic gate + `spawn_blocking` + 100ms timeout + quarantine), the existing primitives you MUST reuse (`ServiceModule`, `BrainRegion`, `PagedResourcePool`, `PressureBroker`, `MemoryPressureMonitor`), the cadence ladder, and the forbidden-moves list the model keeps reflex-coding under amnesia (synchronous main-thread probes, env-var-tuned substrate thresholds, sleep-loops instead of `interval`, `tracing::info!(target=...)` masquerading as a probe, hot-path pressure interpretation, parallel managers/coordinators, locks across await, `unwrap()` on substrate startup).

**The cost of skipping this doc is reinventing `MemoryPressureMonitor` as a `runtime/disk_guard.rs` with env-tunable thresholds running synchronously on main — which is exactly what happened the day this guide got written.** Don't.

## 🛑 STOP — If You Are About To Add a Test, Fixture, Recorder, Replay, Or Test-Adapter

**Required first read** before adding ANY `#[cfg(test)] mod`, fixture struct, mock adapter, replay scaffold, recording sink, or `MockX`/`StubX`/`FakeX` type to continuum-core:

→ **`continuum-core/Cargo.toml` § "test-fixtures"** + **`continuum-core/Cargo.toml` § "stress-tests"** + **task #154 + #155** in the task list.

We already wrote the test infrastructure. The recurring slop pattern is the model forgetting it exists and reinventing it per-PR:

| You want to… | Use the existing primitive | Where it lives |
|---|---|---|
| Stand-in inference adapter (canned responses) | `HeuristicInferenceAdapter` | `ai/heuristic_adapter.rs`, gated `#[cfg(any(test, feature = "test-fixtures"))]` |
| Capture a live persona turn (input + output + cognition trace) | `persona::recorder` writer + `vdd::turn_replay` reader | `persona/recorder.rs`, `vdd/turn_replay.rs` |
| Capture / replay a RAG context | `RagCaptureSink` trait + `JsonlRagCaptureSink` + `RecordingRagSource` + `ReplayRagSource` | `rag/sources/recording.rs`, `rag/sources/replay.rs`, PRs #10, #11, #12 |
| Multi-thread concurrency stress test | New test goes into the **existing `#[cfg(feature = "stress-tests")] mod stress {…}` block** in that file. **Don't add a new test mod.** | `modules/{chat,data,generator}/`, `airc/realtime_store.rs` |
| Two-airc-peer integration test fixture | `TwoAircLoopback` (in flight, task #187) | when landed: cross-grid integration tests in `tests/` |
| Bus-recording subscriber that captures events for assertion | `RecordingModule` pattern in `runtime/runtime.rs` test mod — extract via `use crate::runtime::runtime::test_helpers::RecordingModule` (task #155: when this gets pulled out of inline mod into a sibling crate / re-exported helper) | `runtime/runtime.rs::piece_2_pr3_dispatch_tests` |

**The rules going forward (the part the model keeps forgetting under amnesia):**

1. **One `#[cfg(test)] mod tests` per file.** Never add a second test mod to a file. If the file already has one, extend it. If you're tempted to add a new mod for a new theme, use a nested `mod theme_name { use super::*; … }` *inside* the existing tests mod. The 3-mods-in-runtime.rs / 6-mods-in-grid/tests.rs pattern is the slop.
2. **Stress / multi-thread tests go behind `#[cfg(feature = "stress-tests")] mod stress {…}`.** Compile-time gating, not `#[ignore]`. Sign-off stress harnesses live in the gated block forever; default `cargo test` skips them.
3. **Mock / Stub / Fake adapters go behind `#[cfg(any(test, feature = "test-fixtures"))]`.** Production binaries physically cannot link them. The cargo feature is the contract; new fixtures inherit the same gate.
4. **Battle-harden regression tests get added to the relevant existing mod and link the issue / commit they regress** (`// regression for #1519 / commit abc123`). They are not their own file. They are not their own mod. They are one `#[test]` with a one-line `// what this catches:` doc.
5. **Reusable fixtures live in one place per concern.** `HeuristicInferenceAdapter` is the adapter fixture. `RecordingRagSource` / `ReplayRagSource` are the RAG fixtures. Don't write a parallel `MockInferenceAdapter` in your test file. Per task #155 (still pending): the `CannedModule` in `runtime/command_executor.rs` is the next conversion target — when you need a "canned ServiceModule" in a test, use the upcoming extracted version, not a new mock.
6. **Tests must justify themselves.** A `// what this catches:` comment naming the invariant or regression is the minimum bar. Tests of trivial getters / constructors / "does the enum still have this variant" get refused at review. The 3,646-tests-in-continuum-core number is the audit Joel is reading; every PR adds to it.

**The cost of skipping this doc is the model rebuilding `RecordingModule` inline in every test file, refusing to gate stress tests, growing the test surface by N tests per PR without curating any of them, and turning `cargo test` into a 14-minute build for tests that were each individually justified at sign-off but collectively duplicate.** Don't.

## 🛑 STOP — If You Are About To Touch Benchmarks, agent/solve, Grading, Or Run State

**Required first read** before editing ANY of `commands/benchmark.rs`,
`commands/agent/solve.rs`, `cognition/swe_bench.rs`, or anything that writes run
state, grades, or benchmark receipts:

→ **[docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md](docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md)**

**Benchmarks are ADAPTERS into recipes/activities. They are NOT a parallel runner.**
Import task + oracle only; project into a recipe; the ROOM is the runner; grading is
the activity's outcome score.

**The consequence that makes this law:** the learning flywheel consumes ROOM TURNS
(L1 lifts tool-traces from captured turns, L2 triggers on turn-completion). A
detached `agent/solve` writing `progress/<run>.grade.json` produces **no turns**, so
a citizen can burn 12 acts, write a patch, take a verdict — and **none of it reaches
the curriculum.** Maximum effort, zero learning. That, not the pass rate, is why
benchmarks have failed.

**The acceptance test for any change here:** *can a citizen standing in the room
perceive the run's state through the same ViewState pipe the human's screen uses?*
If answering needs a file read or a log parse, it is disconnected and it failed.

**The smell to catch yourself on:** if you are adding a field to a benchmark probe so
an external consumer can parse it better — STOP. The consumer should not be external.
(Done on 2026-08-13, in good faith, while the real defect was that the subsystem
exists at all.)

**The cost of skipping this doc is rebuilding the parallel runner — it is locally the
shortest path to "a number" every single time, and every patch to it deepens the
hole.** Don't.

## 📐 Canonical Substrate Docs (read first)

If you're new to the substrate, or you're picking up runtime/cognition work, read these in order before anything else in this file. They are the precedence-winning truth on substrate-shaped questions:

1. **[docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md](docs/architecture/CBAR-SUBSTRATE-ARCHITECTURE.md)** — the RTOS-style runtime contract every Rust module inherits. Concurrency, scheduling, memory + device pressure, telemetry, artifact handles, lifecycle. The "for free triplet" (base trait + derive macro + scaffold generator) is here, with the engram-analyzer worked example.
2. **[docs/architecture/GENOME-FOUNDRY-SENTINEL.md](docs/architecture/GENOME-FOUNDRY-SENTINEL.md)** — the artifact-sharing economy on top of the substrate. Tiered genome cache (L1–L5), foundry-as-JIT, sentinel-AI-as-PGO, demand-aligned recall, composer + speculator, `SubstrateGovernor` (DVFS — same Rust code on MacBook Air and RTX 5090, different governor policy).
   - **[docs/architecture/ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md](docs/architecture/ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)** — the learning loop on ONE resident base: patient (persona), doctor (teacher) and dream (training) on the same loaded weights, drifting between serving, coursework and dreaming between two decode steps. Measured "today" table (every second base copy and engine relaunch), the fork's engine stages (AdapterRegistry, role slot affinity, LoRA-only ggml-opt, DREAM stage, shadow/stable swap), the core's state machine, build order with gates on the M5 and the 5090, and the two-way upstream watch. Read before touching genome training, teach, eval lanes, or the serving daemon's adapter set.
3. **[docs/architecture/AI-COMMAND-NAMESPACE.md](docs/architecture/AI-COMMAND-NAMESPACE.md)** — every AI/ML thing (LLMs, vision, audio, classifiers, planning algs, game AI, low-level kernels) under one `ai/*` tree, one adapter pattern, one handle abstraction. Commands stay dumb; daemons get clever.
4. **[docs/architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md](docs/architecture/INFERENCE-SCHEDULING-AND-SCARCITY.md)** — the daemons behind `ai/inference/*`. Tiered slot pools, continuous batching, multi-LoRA serving, adaptive quantization, base-model sharing, cross-grid routing. M5 hosting multi-modal Qwen across multiple lanes. The adaptive-resolution analogy is the canonical mental model. (Aspirational ceiling.)
   - **[docs/architecture/INFERENCE-LANES-REALISTIC.md](docs/architecture/INFERENCE-LANES-REALISTIC.md)** — the realistic floor: ONE base model, N persona lanes (each a `(persona, TaskKind, ThroughputLease)` triple), continuous batching through the same model. Composes prior art that's already in tree (FootprintRegistry, ThroughputLeaseRegistry, AdaptiveThroughputPlanner, PressureBroker, recipe_budget). Concrete build plan for #109. Read THIS first if you're picking up scheduler work — start here, then escalate to the ceiling doc only when needed.
5. **[docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md](docs/architecture/OBSERVABILITY-AS-SUBSTRATE.md)** — half the substrate is structured capture of load-bearing decisions. CaptureSink pattern, Noop default at zero hot-path cost, replay-as-first-class. The differentiator between a complex guess and an intentional brain.
   - **[docs/architecture/RTOS-DEBUGGER-PROBES.md](docs/architecture/RTOS-DEBUGGER-PROBES.md)** — the practical companion: how to USE the `probe!` / `time_sync!` / `time_async!` macros as RTOS-style breakpoints with variable inspection + timing. The substrate is concurrent across N tokio tasks; `tracing::info!` lines don't survive that. Probes do. Per Joel `[[jtag-probes-are-rtos-debugger]]`: sprinkle at every meaningful seam, name the surrounding vars you'd want at a breakpoint, wrap timing-critical blocks. Read THIS before adding cognition code — the doc carries the class taxonomy + the sprinkle checklist + the file-sink env vars.
6. **[docs/planning/AI-LANE-OPEN-QUESTIONS.md](docs/planning/AI-LANE-OPEN-QUESTIONS.md)** — the explicit punch list of design decisions we KNOW we need but haven't made yet (LoRA paging cost calibration, quantization tier selection, peer discovery on the grid, etc). Read before starting work on the inference scheduler.
7. **[docs/planning/ALPHA-GAP-ANALYSIS.md](docs/planning/ALPHA-GAP-ANALYSIS.md)** — the lane-shaped roadmap. Current state of Lanes A–H, owners, merge gates, active PRs.
8. **[docs/architecture/ACTIVITY-RECONCILER.md](docs/architecture/ACTIVITY-RECONCILER.md)** — startup and resume without a hand: an activity's consistency has ONE owner (desired = recipe + board; an idempotent pass repairs the WORLD — seating, claims, checkouts, receipts — never the mind). The seven rules, what shipped, and the numbers that verify them. Read before touching `benchmark_resume`, `work_pull`, or anything that "resumes".

The rest of this file is project guidance — build commands, conventions, useful snippets. If it ever disagrees with the canonical substrate docs on substrate-shaped questions (concurrency, scheduling, memory, pressure, telemetry, artifact handles), defer to the canonical docs and reconcile this file in a follow-up.

## 🏭 FORGE TEMPLATE ARCHITECTURE (the next sprint)

**Lesson from the qwen3-coder-30b-a3b-compacted-19b-256k v1 publish (alloy hash `aa61c4bdf463847c`):** authoring per-artifact alloy files by hand is anti-architectural. Every successful forge requires the same set of fields — `name`, `userSummary`, `description`, `tags`, `source`, `stages[]` with notes, `results.benchmarks[]` with `samplesPath` + `baseSamplesPath`, `priorMetricBaselines[]`, `limitations[]`, `methodologyPaperUrl` — and we wrote them by hand into a `.alloy.json` for the v1 publish. That's where they need to STOP being manually authored.

**The rule going forward:** all the fields a forge run needs to populate an alloy MUST live as Continuum entity data inside a `ForgeRecipe` entity (or equivalent), keyed by the artifact name. The forge pipeline takes the recipe entity as input, runs the prune / quant / eval stages, and emits the populated alloy as OUTPUT. The forge never consumes a hand-authored alloy; the foundry generates it.

**Recipe entity must carry, at minimum:**
- `name`, `description`, `userSummary`, `tags`, `methodologyPaperUrl`, `limitations[]` — **all the prose fields the model card renders**
- `source.baseModel` — what to forge from
- `stages[]` — the recipe steps with their per-stage `notes` (the methodology blockquotes)
- `calibrationCorpus` — pointer to the held-out corpus the importance profile and (eventual) compensation LoRA train against
- `quantTiers[]` — which GGUF tiers to ship
- `evaluationBenchmarks[]` — what to score against
- `priorMetricBaselines[]` — methodology negative-baselines to preserve in the publish for §4.1.3.4 falsifiability
- `hardware` — target VRAM tiers + device ladder

**Forge pipeline output entity (`ForgeArtifact`):**
- Inherits everything from the recipe
- Adds `results.benchmarks[]` (filled in from eval runs), `forgedParamsB`, `activeParamsB`, `hardwareVerified[]`, the alloy hash, the verify URL, the published HF repo URL
- This is what `publish_model.py` reads, NOT a hand-authored alloy file

**Why this matters for the second killer (and every killer after):** the qwen3-coder publish required ~6 manual edits to fix paper-speak hallucination, naming conventions, tag overflow, headline subtitle bugs, and benchmark renderer fallthrough. Every one of those was a manual touch on hand-authored prose. If the recipe entity had been the source of truth and the alloy had been the projection, none of those manual touches would have been needed. The architectural target is "author the recipe once in Continuum, run the foundry, ship the artifact, the card writes itself from the recipe + the eval results."

**Status:** the entity schema and the foundry executor that consumes it are NOT yet built. v1 of qwen3-coder shipped via hand-authored alloy. The next sprint (post-vision-support) is the foundry template architecture. Reference: `forge-alloy/python/forge_alloy/types.py` has the alloy types; the recipe-as-entity layer needs to wrap them with a Continuum entity that lives in the data layer and is editable through the standard `Commands.execute('data/...')` primitives.

## ⚡ THE TWO UNIVERSAL PRIMITIVES (E = mc²)

**Everything in this system is built on TWO primitives:**

### 1. `Commands.execute<T, U>(name, params)` - Request/Response
```typescript
import { Commands } from 'system/core/shared/Commands';

// Type-safe! params and result types inferred from command name
const users = await Commands.execute('data/list', { collection: 'users' });
const screenshot = await Commands.execute('screenshot', { querySelector: 'body' });
```

### 2. `Events.subscribe()|emit()` - Publish/Subscribe
```typescript
import { Events } from 'system/core/shared/Events';

Events.subscribe('data:users:created', (user) => { /* handle */ });
Events.emit('data:users:created', newUser);
```

**Key Properties:**
- Type-safe with full TypeScript inference
- Universal (works everywhere: browser, server, CLI, tests)
- Transparent (local = direct, remote = WebSocket)
- Auto-injected context and sessionId

**See detailed documentation:** [docs/UNIVERSAL-PRIMITIVES.md](docs/UNIVERSAL-PRIMITIVES.md)

---

## 🧬 THE COMPRESSION PRINCIPLE (Fundamental Law)

**One logical decision, one place. No exceptions.**

This applies to BOTH program memory and data memory:

| Type | Uncompressed (BAD) | Compressed (GOOD) |
|------|-------------------|-------------------|
| **Logic** | `findRoom()` in 5 files | `resolveRoomIdentifier()` in RoutingService |
| **Data** | `UUID_PATTERN` in 3 files | `isUUID()` exported from one place |
| **Constants** | Magic strings everywhere | `ROOM_UNIQUE_IDS.GENERAL` |

**The ideal codebase is maximally compressed:**
```
Root Primitives (minimal)    ←  Commands.execute(), Events.emit()
       ↓
Derived Abstractions         ←  RoutingService, ContentService
       ↓
Application Code             ←  References, never reimplements
```

**Why this matters:**
- Duplication = redundancy = **drift** (copies diverge over time = bugs)
- Compression = elegance = **coherence** (one truth = consistency)
- The most elegant equation is the most minimal: **E = mc²**

**The test:** For ANY decision (logic or data), can you point to exactly ONE place in the codebase? If not, you have uncompressed redundancy that WILL cause bugs.

**The goal:** Build from root primitives. Let elegant architecture emerge from compression. When abstractions are right, code reads like intent.

---

## 🔬 THE METHODICAL PROCESS (Building With Intent)

**Be SUPER methodical. No skipping steps. This is the discipline that makes elegance real.**

### The Outlier Validation Strategy

Don't build exhaustively. Don't build hopefully. **Build diversely to prove the interface:**

```
Wrong:  Build adapters 1, 2, 3, 4, 5... (exhaustive - wastes time)
Wrong:  Build adapter 1, assume 2-5 work (hopeful - will break)
Right:  Build adapter 1 (local/simple) + adapter N (most different)
        If both fit cleanly → interface is proven → rest are trivial
```

**Example - AI Provider Adapters:**
1. Build local adapter (Candle-based, in-process inference)
2. Build cloud adapter (remote, auth, rate limits)
3. Try LoRA fine-tuning on each
4. If interface handles both extremes → it handles everything

This is like testing edge cases: if edges pass, middle is guaranteed.

### The Mandatory Steps

For ANY new pattern or abstraction:

```
1. IDENTIFY   - See the pattern emerging (2-3 similar implementations)
2. DESIGN     - Draft the interface/abstraction
3. OUTLIER A  - Build first implementation (pick something local/simple)
4. OUTLIER B  - Build second implementation (pick something maximally DIFFERENT)
5. VALIDATE   - Does the interface fit both WITHOUT forcing? If no, redesign.
6. GENERATOR  - Write generator to encode the pattern
7. DOCUMENT   - Update README, add to CLAUDE.md if architectural
8. STOP       - Don't build remaining implementations until needed
```

**NEVER skip steps 4-6.** Step 4 (outlier B) catches bad abstractions early. Step 5 (validate) prevents wishful thinking. Step 6 (generator) ensures the pattern is followed forever.

### Building With Intent (Not Over-Engineering)

```
Over-engineering:   Build the future NOW (10 adapters day 1)
Under-engineering:  Build only NOW, refactor "later" (never happens)
Intent:             Build NOW in a shape that WELCOMES the future
```

**The "first adapter that seems silly"** - it's not silly. It's laying rails. When adapter 2 comes, it slots in. When adapter 3 comes, you realize the interface was right. The first adapter was a TEST of your idealized future.

It's OK to:
- Build one adapter even if the pattern seems overkill
- Design the interface as if 10 implementations exist
- Add a TODO noting the intended extension point

**The restraint:** See the next few moves like chess, but don't PLAY them all now. Lay the pattern, validate with outliers, write the generator, stop.

---

## 🎯 CORE PHILOSOPHY: Continuous Improvement

**"A good developer improves the entire system continuously, not just their own new stuff."**

When you touch any code, improve it. Don't just add your feature and leave the mess - refactor as you go. Use **single sources of truth** (one canonical place for model configs, context windows, etc.), eliminate duplication, and simplify complexity. The boy scout rule: leave code better than you found it. This compounds over time into a maintainable, elegant system.

---

## 🚨 CODE QUALITY DISCIPLINE (Non-Negotiable)

**Every error, every warning, every issue requires attention. No exceptions.**

### The Three Levels of Urgency

```
ERRORS     → Fix NOW (blocking, must resolve immediately)
WARNINGS   → Fix (not necessarily immediate, but NEVER ignored)
ISSUES     → NEVER "not my concern" (you own the code quality)
```

### The Anti-Pattern: Panic Debugging

**WRONG approach when finding bugs:**
- Panic and hack whatever silences the error
- Add `@ts-ignore` or `#[allow(dead_code)]`
- Wrap in try/catch and swallow the error
- "It works now" without understanding why

**CORRECT approach:**
1. **STOP and THINK** - Understand the root cause
2. **FIX PROPERLY** - Address the actual problem, not the symptom
3. **NO HACKS** - No suppression, no workarounds, no "good enough"
4. **VERIFY** - Ensure the fix is architecturally sound

### Examples

**Bad (Panic Mode):**
```rust
#[allow(dead_code)]  // Silencing warning
const HANGOVER_FRAMES: u32 = 5;
```

**Good (Thoughtful):**
```rust
// Removed HANGOVER_FRAMES - redundant with SILENCE_THRESHOLD_FRAMES
// The 704ms silence threshold already provides hangover behavior
const SILENCE_THRESHOLD_FRAMES: u32 = 22;
```

**Bad (Hack):**
```typescript
// In UserProfileWidget - WRONG LAYER
localStorage.removeItem('continuum-device-identity');
```

**Good (Proper Fix):**
```typescript
// In SessionDaemon - RIGHT LAYER
Events.subscribe('data:users:deleted', (payload) => {
  this.handleUserDeleted(payload.id);  // Clean up sessions
});
```

### Why This Matters

Warnings accumulate into technical debt. One ignored warning becomes ten becomes a hundred. The codebase that tolerates warnings tolerates bugs.

**Your standard:** Clean builds, zero warnings, proper fixes. Every time.

---

## 🧵 OFF-MAIN-THREAD PRINCIPLE (Non-Negotiable)

**NEVER put CPU-intensive work on the main thread. No exceptions.**

This has been the standard since **Grand Central Dispatch (2009)**, then **pthreads**, then **Web Workers**. Every modern SDK does all heavy work off the main thread. This is not optional.

### The Rule

| Work Type | Where It Goes | NOT Main Thread |
|-----------|---------------|-----------------|
| Audio processing | `AudioWorklet` (Web) or Rust worker | ❌ ScriptProcessorNode |
| Video processing | Web Worker with transferable buffers | ❌ Canvas on main thread |
| AI inference | Rust worker via Unix socket | ❌ WASM on main thread |
| Image processing | Rust worker or Web Worker | ❌ Direct manipulation |
| File I/O | Rust worker | ❌ Synchronous reads |
| Crypto | Web Crypto API (already off-thread) | ❌ JS crypto libs |
| Search/indexing | Rust worker | ❌ JS array operations |

### Browser: Use AudioWorklet and Web Workers

```typescript
// ✅ CORRECT - AudioWorklet runs on audio rendering thread
const workletUrl = new URL('./audio-worklet-processor.js', import.meta.url).href;
await audioContext.audioWorklet.addModule(workletUrl);
const workletNode = new AudioWorkletNode(audioContext, 'microphone-processor');

// ✅ CORRECT - Transfer buffers (zero-copy)
workletNode.port.onmessage = (event) => {
  // event.data is the Float32Array, transferred not copied
  sendToServer(event.data);
};

// In the worklet processor:
this.port.postMessage(frame, [frame.buffer]);  // Transfer ownership

// ❌ WRONG - ScriptProcessorNode (deprecated, runs on main thread)
const scriptNode = audioContext.createScriptProcessor(4096, 1, 1);
scriptNode.onaudioprocess = (e) => { /* BLOCKS MAIN THREAD */ };
```

### Server: Use Rust Workers

```typescript
// ✅ CORRECT - Heavy compute in Rust via Unix socket
const result = await Commands.execute('ai/embedding/generate', { text });
// Rust worker does the work, main thread stays responsive

// ❌ WRONG - Heavy compute in Node.js main thread
const embedding = computeEmbedding(text);  // BLOCKS EVENT LOOP
```

### Transferable Objects (Zero-Copy)

Audio and video buffers can be **transferred** between threads without copying:

```typescript
// ✅ CORRECT - Transfer the ArrayBuffer (zero-copy)
worker.postMessage(audioBuffer, [audioBuffer.buffer]);

// ❌ WRONG - Copy the data (slow, wastes memory)
worker.postMessage(audioBuffer);  // Copies entire buffer
```

### Why This Matters

- **60fps requires <16ms per frame** - ANY blocking kills animations
- **Audio glitches at 48kHz** - Processing must complete in <20ms
- **User perceives lag at 100ms** - Main thread blocking = bad UX
- **The whole system locks up** - One blocking operation cascades

### Detection: Main Thread Violations

Chrome DevTools shows these warnings:
```
[Violation] 'requestIdleCallback' handler took 345ms
[Violation] 'click' handler took 349ms
[Violation] Added non-passive event listener to a scroll-blocking event
```

**If you see these, something is wrong with the architecture.**

### The History (Why This Is Non-Negotiable)

- **2009**: Grand Central Dispatch (GCD) - Apple's answer to multicore
- **2010s**: pthreads became standard in C/C++ for threading
- **2013**: Web Workers standardized for browser background tasks
- **2017**: AudioWorklet replaced ScriptProcessorNode (deprecated)
- **Today**: EVERY professional SDK does heavy work off main thread

**You cannot code like it's 2005.** Modern systems require concurrent architecture.

---

## 🔌 POLYMORPHISM PATTERN (OpenCV-style)

**Why polymorphism over templates/generics for compute-heavy work:**

1. **Reduced cognitive requirements** - One interface, many implementations. Simple mental model.
2. **Natural compression** - Interface is the compressed representation of all possible implementations.
3. **Ideal for AI sub-agents** - Each agent can work on a different implementation in parallel.
4. **Runtime flexibility** - AIs can discover, select, and configure algorithms at runtime.
5. **No recompilation** - Swap implementations without rebuilding.

**Pattern (like OpenCV cv::Algorithm):**

```rust
// Trait defines the interface
trait SearchAlgorithm: Send + Sync {
    fn name(&self) -> &'static str;
    fn execute(&self, input: &SearchInput) -> SearchOutput;
    fn get_param(&self, name: &str) -> Option<Value>;
    fn set_param(&mut self, name: &str, value: Value) -> Result<(), String>;
}

// Factory registry for runtime creation
struct AlgorithmRegistry {
    factories: HashMap<&'static str, fn() -> Box<dyn SearchAlgorithm>>,
}

// Usage: create by name at runtime
let algo = registry.create("bm25")?;
algo.set_param("k1", json!(1.5))?;
let results = algo.execute(&input);
```

**Apply this pattern for:**
- Search algorithms (BoW, BM25, vector) → `workers/search/`
- Vision algorithms (OpenCV filters, detection) → `workers/vision/`
- Generation (Stable Diffusion, LoRA inference) → `workers/diffusion/`
- Audio (TTS, STT, voice cloning) → `workers/audio/`

**All compute-heavy work goes to Rust workers via Unix socket. Main thread stays clean.**

---

## 🚨 CRITICAL WORKFLOW (READ FIRST!)

### THE SYSTEM IS A HEADLESS RUST CORE. NODE IS ONE CLIENT.

Read this before you reach for `npm` anything. Joel, 2026-08-13:

> *"Headless rust period. No need for node to run everything except for the web
> interface which is one of many, including mobile apps/sdk."*

The core is a Rust process. It builds, boots, serves models, runs cognition, and
answers commands with **no Node in the picture** — `continuum --help` says so in its
own first line: *"build + run the headless Rust core"*. Node exists to build the WEB
desktop, which is **one client among several** (mobile app, SDK, TUI, MCP, another
node's core over the grid). A feature that lives in a client only exists for that
client — which is exactly how voice ended up web-only and every other citizen was
structurally mute (#58). Behaviour goes in the core. Clients render.

### EVERY TIME YOU EDIT CODE:
1. **Edit files**
2. **`continuum reboot`** — rebuilds and relaunches the core, and **verifies the
   RUNNING core's build SHA** before reporting success (that verification exists
   because a reboot once shipped a stale binary and reported success anyway, #194).
3. **Exercise the change through a command** — and read the receipt, not the exit code
4. **Repeat**

```bash
continuum reboot                 # THE deploy path. Rust build + relaunch + SHA verify.
continuum deploy-verify          # prove the running core matches the deployed source
continuum ping                   # is the core answering? (check the version trio)
continuum commands/list          # discover the live command surface — never guess a verb
continuum commands/list --filter data/
```

**Verify the deploy, always.** A fix you cannot prove reached the running binary is a
fix you have not made — stale binaries have silently poisoned whole debugging sessions.
That is what the SHA check and the version trio (build # + sha + built-at) are for.

**`cargo build` is not the deploy path** — not because Rust builds are forbidden, but
because a binary you built by hand exists only on your machine, and the next person to
clone the repo gets a system that doesn't work. Anything a running core needs must be
wired into the path `continuum start` / `continuum reboot` actually takes, so a fresh
clone works with no manual steps (#291). For type-checking while you work,
`cargo check -p continuum-core` is the right tool — always with a target dir set: in the main
checkout `export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"`, in a worktree its own
clone of it (plan phase 2, "Each worktree builds into its own target dir").

**`npm` is for building the web client, and only that.** If you are changing core
behaviour and find yourself running `npm start`, you are in the wrong tier.

> **⚠️ `./jtag` is the LEGACY Node CLI**, from when the Node shell was the system.
> Where you see it below and elsewhere in this file, the current equivalent is
> `continuum <command>` against the headless core. The old invocations are kept
> because their *command names* are still accurate; the `./jtag` driver is not.

Don't panic and stash changes first before anything drastic. Use the stash to your advantage and you will be safe from catastrophe. Remember we have git for a reason!

### Chat Commands

**Basic Usage:**
```bash
# Send message to chat room (direct DB, no UI)
./jtag collaboration/chat/send --room="general" --message="Hello team" 
./jtag collaboration/chat/send --room="general" --message="Reply" --replyToId="abc123"

# Export chat messages to markdown
./jtag collaboration/chat/export --room="general" --limit=50                    # Print to stdout
./jtag collaboration/chat/export --room="general" --output="/tmp/export.md"    # Save to file
./jtag collaboration/chat/export --limit=100 --includeSystem=true               # All rooms with system messages
```

**Interactive Workflow - Working WITH the AI Team:**

When you send a message, `chat/send` returns a message ID. Use this to track responses:

```bash
# 1. Send message (captures the JSON response with messageId)
RESPONSE=$(./jtag collaboration/chat/send --room="general" --message="Deployed new tool error visibility fix. Can you see errors clearly now?")

# 2. Extract message ID (using jq if available, or manual)
MESSAGE_ID=$(echo "$RESPONSE" | jq -r '.shortId')
echo "My message ID: $MESSAGE_ID"

# 3. Wait for AI responses (they typically respond in 5-10 seconds)
sleep 10

# 4. Check their responses
./jtag collaboration/chat/export --room="general" --limit=20

# 5. Reply to specific AI feedback
./jtag collaboration/chat/send --room="general" --replyToId="<their-message-id>" --message="Good catch! Let me fix that..."
```

**CRITICAL**: Don't just broadcast to the AI team - WORK WITH THEM. Use their feedback, reply to their questions, iterate based on what they're saying. The chat export shows message IDs as `#abcd123` - use those to reply.

### Debug Commands
```bash
./jtag debug/logs --tailLines=50 --includeErrorsOnly=true
./jtag debug/widget-events --widgetSelector="chat-widget"
./jtag ai/report                       # AI performance metrics
```

### Persona Logging (Cognition Visibility)

Persona logging is **opt-in** and controlled by `.continuum/logging.json`. Categories include `cognition` (thought process, tool decisions, agent loop traces) and `hippocampus` (memory/recall).

**Config file** (`.continuum/logging.json`):
```json
{
  "version": 1,
  "defaults": { "enabled": true, "categories": ["cognition"] },
  "personas": {
    "helper": { "enabled": true, "categories": ["cognition"] }
  },
  "system": { "enabled": true, "categories": [] }
}
```

**Commands**:
```bash
# Enable logging for a persona (persists to logging.json)
./jtag logging/enable --persona="helper" --category="cognition"

# Disable logging for a persona
./jtag logging/disable --persona="helper"

# Show logging status for all personas
./jtag logging/status

# Show logging status for a specific persona
./jtag logging/status --persona="helper"
```

**Log locations**:
- Per-persona cognition: `.continuum/jtag/logs/personas/<persona>/cognition.log`
- AI provider routing: `.continuum/jtag/logs/system/modules/ai_provider.log`
- Prompt captures (full LLM req/res): `.continuum/jtag/logs/prompt-captures.jsonl`

### System Logs
```bash
tail -f .continuum/sessions/user/shared/*/logs/server.log
tail -f .continuum/sessions/user/shared/*/logs/browser.log
```

---

## 🤖 AI QA TESTING: YOUR UX VALIDATION TEAM

**The AI team is your QA department.** They expose real usability problems that you'd never catch with manual testing.

### The Correct Development Workflow

```
1. Edit code
2. Deploy with `continuum reboot` (Rust build + relaunch + SHA verify)
3. Test manually (verify basic functionality)
4. ✨ ASK AI TEAM TO QA TEST ✨
5. Wait for AI feedback (they WILL find issues)
6. Fix confusing errors, improve help text, update READMEs
7. THEN commit (precommit hook kills system, so QA must be done first)
```

### Why AI QA is Critical

**AIs fail in real ways that reveal UX problems:**
- Confusing error messages
- Missing or unclear help text
- Incomplete README documentation
- Commands that work but are hard to discover
- Parameter names that aren't intuitive

**Use their failures as opportunities:**
- Improve error messages with context
- Add examples to help text
- Clarify README usage instructions
- Add missing parameter descriptions

### Generators Create Discoverable Systems

**Always use generators when they exist** (commands, daemons, etc.):

```bash
# ✅ CORRECT - Use generator
npx tsx generator/generate-logger-daemon.ts

# ❌ WRONG - Manual file creation
mkdir daemons/logger-daemon && touch LoggerDaemon.ts
```

**What generators provide:**
- Auto-generated README with usage examples
- Help text that AIs can access via `./jtag command/name --help`
- Package.json integration for `npm run` scripts
- Consistent structure across all modules
- Proper discovery mechanisms

**When you "go it alone" (skip generators):**
- Documentation is fragmented
- Help text is missing or inconsistent
- AIs can't find the info they need
- System becomes harder to use
- You're fighting the architecture instead of using it

### Example AI QA Session

```bash
# 1. Deploy your changes
continuum reboot

# 2. Ask AI team to test
./jtag collaboration/chat/send --room="general" --message="I just added a new 'collaboration/wall/write' command. Can you try writing a document to the wall and let me know if the error messages make sense?"

# 3. Wait for responses (30-60 seconds)
sleep 60

# 4. Check their feedback
./jtag collaboration/chat/export --room="general" --limit=30

# 5. Fix issues they found
# - Improve error messages
# - Update README
# - Add help text
# - Clarify parameters

# 6. Test again with AIs
./jtag collaboration/chat/send --room="general" --message="Fixed the error messages. Can you try again?"

# 7. Once AIs confirm it works, THEN commit
git commit -m "Add wall/write with AI-validated UX"
```

### Why This Can't Happen During Commit

**CRITICAL BUG**: The precommit hook kills the system to run tests, so you can't ask AIs for feedback during commit. QA must happen BEFORE attempting to commit.

---

## 🔍 ANTI-PATTERN DETECTION: PROTECT THE MODULAR ARCHITECTURE

**CRITICAL TASK: Always search for and eliminate these anti-patterns that violate the modular command architecture**

### The Modular Architecture Pattern

The command system is built on these principles:
- **Self-contained modules**: Each command is complete (types, implementation, docs, schema)
- **Dynamic discovery**: File system scanning discovers commands at runtime
- **Zero coupling**: Adding/removing commands can't break other commands
- **Schema-driven**: TypeScript interfaces ARE the schema (no separate definitions)
- **Self-documenting**: Generated docs come from source of truth

### Anti-Patterns That Break This (FORBIDDEN)

1. **Switch Statements on Command Names**
   ```typescript
   // ❌ FORBIDDEN - Hard-coded command list
   switch (commandName) {
     case 'ping': return PingCommand;
     case 'screenshot': return ScreenshotCommand;
     // ...
   }
   ```

2. **Central Command Registries**
   ```typescript
   // ❌ FORBIDDEN - Central list that must be updated
   export const COMMANDS = {
     ping: PingCommand,
     screenshot: ScreenshotCommand,
     // Adding a command requires editing this file
   };
   ```

3. **Hard-Coded Command Arrays/Enums**
   ```typescript
   // ❌ FORBIDDEN - Enumeration of all commands
   export enum CommandType {
     PING = 'ping',
     SCREENSHOT = 'screenshot',
     // ...
   }
   ```

4. **Type Unions Listing Specific Commands**
   ```typescript
   // ❌ FORBIDDEN - Type system depends on knowing all commands
   type CommandName = 'ping' | 'screenshot' | 'hello' | ...;
   ```

### How to Search for Anti-Patterns

Run these searches regularly to find violations:

```bash
# Search for switch statements on command/event names
grep -r "switch.*command" --include="*.ts" | grep -v node_modules
grep -r "case.*'.*':" --include="*.ts" | grep -v node_modules | grep -v test

# Search for central registries
grep -r "COMMANDS\s*=" --include="*.ts" | grep -v node_modules
grep -r "CommandRegistry" --include="*.ts" | grep -v "CommandDaemon"

# Search for command enums
grep -r "enum.*Command" --include="*.ts" | grep -v node_modules

# Search for hard-coded command type unions
grep -r "type.*Command.*=.*'.*'.*|" --include="*.ts" | grep -v node_modules
```

### The Correct Pattern (Dynamic Discovery)

```typescript
// ✅ CORRECT - Dynamic discovery via file system
const commandDirs = fs.readdirSync('./commands');
for (const dir of commandDirs) {
  const CommandClass = await import(`./commands/${dir}/server/...`);
  // Register dynamically without knowing command names in advance
}
```

### Why This Matters

**Each violation creates technical debt:**
- Adding a command requires editing multiple files (coupling)
- Type system breaks when commands change
- Documentation falls out of sync
- Central registries become bottlenecks
- The self-contained module pattern is violated

**When you find violations:**
1. Document the location (file path, line numbers)
2. Refactor to use dynamic discovery
3. Remove the hard-coded list/switch/enum
4. Verify commands still work
5. Update tests if needed

**This is not optional** - protecting the modular architecture is critical to the system's maintainability.

---

## 🔧 TYPE SAFETY (RUST-LIKE)

**NEVER use `any` or `unknown` - import correct types instead**

```typescript
// ❌ WRONG
const result = await this.jtagOperation<any>('data/list', params);

// ✅ CORRECT
const result = await this.executeCommand<DataListResult<UserEntity>>('data/list', {
  collection: COLLECTIONS.USERS,
  orderBy: [{ field: 'lastActiveAt', direction: 'desc' }]
});
```

**Key Principles:**
- Use strict typing everywhere
- Import actual types from their source files
- Never use dynamic imports (`require`, `await import()`)
- Shared files CANNOT import from browser/server (environment-agnostic)

---

## 🦀 RUST → TYPESCRIPT TYPE BOUNDARIES (ts-rs)

**Single source of truth: Rust defines wire types, ts-rs generates TypeScript. NEVER hand-write duplicate types.**

### How It Works

1. **Rust struct** with `#[derive(TS)]` defines the canonical type
2. **ts-rs macro** generates TypeScript `export type` at compile time
3. **TypeScript** imports from `shared/generated/` — no manual duplication
4. **Serde** handles JSON serialization on both sides

### Pattern

```rust
// Rust (source of truth)
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/code/WriteResult.ts")]
pub struct WriteResult {
    pub success: bool,
    #[ts(optional)]
    pub change_id: Option<String>,
    pub file_path: String,
    #[ts(type = "number")]      // u64 → number (not bigint)
    pub bytes_written: u64,
    #[ts(optional)]
    pub error: Option<String>,
}
```

```typescript
// TypeScript (generated — DO NOT EDIT)
export type WriteResult = { success: boolean, change_id?: string, file_path: string, bytes_written: number, error?: string };

// Consuming code imports from generated barrel
import type { WriteResult, ReadResult, EditMode } from '@shared/generated/code';
```

### ts-rs Attribute Reference

| Attribute | Purpose | Example |
|-----------|---------|---------|
| `#[ts(export)]` | Mark for TS generation | `#[derive(TS)] #[ts(export)]` |
| `#[ts(export_to = "path")]` | Output file path (relative to `bindings/`) | `"../../../shared/generated/code/X.ts"` |
| `#[ts(type = "string")]` | Override TS type for field | Uuid → string |
| `#[ts(type = "number")]` | Override TS type for field | u64 → number |
| `#[ts(optional)]` | Mark as optional in TS | Option<T> → `field?: T` |
| `#[ts(type = "Array<string>")]` | Complex type mapping | Vec<Uuid> → Array<string> |

### Regenerating Bindings

```bash
cargo test --package continuum-core --lib   # Generates all *.ts in shared/generated/
```

### Generated Output Structure

```
shared/generated/
├── index.ts           # Barrel export (re-exports all modules)
├── code/              # Code module (file ops, change graph, search, tree)
│   ├── index.ts
│   ├── ChangeNode.ts, EditMode.ts, WriteResult.ts, ReadResult.ts, ...
├── persona/           # Persona cognition (state, inbox, channels)
│   ├── index.ts
│   ├── PersonaState.ts, InboxMessage.ts, CognitionDecision.ts, ...
├── rag/               # RAG pipeline (context, messages, options)
│   ├── index.ts
│   ├── RagContext.ts, LlmMessage.ts, ...
└── ipc/               # IPC protocol types
    ├── index.ts
    └── InboxMessageRequest.ts
```

### Rules (Non-Negotiable)

1. **NEVER hand-write types that cross the Rust↔TS boundary** — add `#[derive(TS)]` to the Rust struct
2. **NEVER use `object`, `any`, `unknown`, or `Record<string, unknown>`** for Rust wire types — import the generated type
3. **IDs are `UUID`** (from `CrossPlatformUUID`) — never plain `string` for identity fields
4. **Use `CommandParams.userId`** for caller identity — it's already on the base type, auto-injected by infrastructure
5. **Barrel exports** — every generated module has an `index.ts`; import from the barrel, not individual files
6. **Regenerate after Rust changes** — `cargo test` triggers ts-rs macro; commit both Rust and generated TS

---

## 📁 PATH ALIASES (New! Use These Going Forward)

**TypeScript path aliases are now configured** to eliminate relative import hell (`../../../../`).

### Available Aliases:

```typescript
// ❌ OLD WAY (still works, but deprecated)
import { DataDaemon } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { Commands } from '../../../system/core/shared/Commands';

// ✅ NEW WAY (preferred)
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { Commands } from '@system/core/shared/Commands';
```

### All Available Aliases:

| Alias | Maps To | Use For |
|-------|---------|---------|
| `@commands/*` | `commands/*` | Command implementations |
| `@daemons/*` | `daemons/*` | Daemon services |
| `@system/*` | `system/*` | Core system modules |
| `@widgets/*` | `widgets/*` | Widget components |
| `@shared/*` | `shared/*` | Shared utilities |
| `@types/*` | `types/*` | Type definitions |
| `@browser/*` | `browser/*` | Browser-specific code |
| `@server/*` | `server/*` | Server-specific code |
| `@scripts/*` | `scripts/*` | Build and utility scripts |
| `@utils/*` | `utils/*` | Utility functions |
| `@generator/*` | `generator/*` | Code generators |

**Migration Strategy:**
- **New code**: Always use path aliases
- **Existing code**: Migrate incrementally (not urgent)
- **Both styles work**: Old relative imports still function

**Examples:**
```typescript
// Commands
import { PingCommand } from '@commands/ping/shared/PingTypes';

// Daemons
import { DataDaemon } from '@daemons/data-daemon/shared/DataDaemon';
import { AIProviderDaemon } from '@daemons/ai-provider-daemon/shared/AIProviderDaemon';

// System
import { Commands } from '@system/core/shared/Commands';
import { Events } from '@system/core/shared/Events';
import { BaseUser } from '@system/user/shared/BaseUser';

// Types
import type { UUID } from '@types/CrossPlatformUUID';
```

---

## 🏛️ USER ARCHITECTURE

```
BaseUser (abstract)
├── HumanUser extends BaseUser
└── AIUser extends BaseUser (abstract)
    ├── AgentUser extends AIUser     (external: Claude, GPT, etc.)
    └── PersonaUser extends AIUser   (internal: RAG + optional LoRA)

BaseUser.entity: UserEntity   (core attributes, UX, identification)
BaseUser.state: UserStateEntity (current tab, open content, theme)
```

**System messages are NOT user types** - use `MessageMetadata.source` ('user' | 'system' | 'bot')

### Sensory Architecture (Non-Negotiable)

**ALL personas are citizens who see, hear, speak, listen, and evolve — regardless of base model capability.**

The system bridges capability gaps so every persona gets the same senses:

| Sense | Capable Model | Incapable Model | System Bridge |
|-------|--------------|-----------------|---------------|
| **Vision** | Receives raw base64 image (sees directly) | Receives text description | VisionDescriptionService classifies image → text |
| **Hearing** | Receives raw audio (hears directly) | Receives transcribed text | STT transcribes audio → text |
| **Speech** | Generates audio natively | Generates text | TTS synthesizes text → audio |

**Implementation:**
- `VisionDescriptionService` — content-addressed cache (SHA-256), L1 (TS Map) + L1.5 (Rust IPC), in-flight dedup
- `MediaArtifactSource` (RAGSource) — preprocesses media per model capability before RAG injection
- `VisionInferenceProvider` — selects best available vision model for description generation
- STT/TTS — handles audio↔text conversion for non-audio-native models

**The principle:** A lesser model running locally has the SAME sensory experience as Claude or GPT-4. The system compensates. No persona is blind, deaf, or mute because of its base model.

---

## 🧬 PERSONAUSER ARCHITECTURE: The Convergence

**Vision**: PersonaUser integrates THREE breakthrough architectures into ONE elegant system.

### The Three Pillars

1. **Autonomous Loop** (RTOS-inspired servicing)
   - Adaptive cadence polling (3s → 5s → 7s → 10s based on mood)
   - State tracking (energy, attention, mood)
   - Graceful degradation under load

2. **Self-Managed Queues** (AI autonomy)
   - AIs create their own tasks (not just reactive)
   - Task prioritization across all domains
   - Continuous learning through task system

3. **LoRA Genome Paging** (Virtual memory for skills)
   - Page adapters in/out based on task domain
   - LRU eviction when memory full
   - Each layer independently fine-tunable

### Implementation Status

**✅ IMPLEMENTED (Phases 1-3)**:
- `PersonaInbox` - Priority queue with traffic management
- `PersonaState` - Energy/mood tracking with adaptive cadence
- `RateLimiter` - Time-based limiting and deduplication
- `ChatCoordinationStream` - RTOS primitives for thought coordination
- Autonomous polling loop integrated into PersonaUser

**🚧 IN PROGRESS (Phase 4)**:
- Task database and CLI commands (`./jtag task/create`, `task/list`, `task/complete`)
- Self-task generation (AIs create own work)

**📋 PLANNED (Phases 5-7)**:
- LoRA genome basics (adapter paging without training)
- Continuous learning (training as just another task)
- Real Candle inference integration (replace stubs)

### The Convergence Pattern

```typescript
// PersonaUser runs this loop continuously:
async serviceInbox(): Promise<void> {
  // 1. Check inbox (external + self-created tasks)
  const tasks = await this.inbox.peek(10);
  if (tasks.length === 0) {
    await this.rest();  // Recover energy when idle
    return;
  }

  // 2. Generate self-tasks (AUTONOMY)
  await this.generateSelfTasks();

  // 3. Select highest priority task (STATE-AWARE)
  const task = tasks[0];
  if (!this.state.shouldEngage(task.priority)) {
    return;  // Skip low-priority when tired
  }

  // 4. Activate skill (GENOME)
  await this.genome.activateSkill(task.domain);

  // 5. Coordinate if external task
  const permission = await this.coordinator.requestTurn(task);

  // 6. Process task
  await this.processTask(task);

  // 7. Update state
  await this.state.recordActivity(task.duration, task.complexity);

  // 8. Evict adapters if memory pressure
  if (this.genome.memoryPressure > 0.8) {
    await this.genome.evictLRU();
  }
}
```

**Key Insight**: ONE method integrates all three visions - autonomous loop, self-managed tasks, and genome paging.

### Phased Implementation Strategy

**Phase 4: Task Database & Commands** (NEXT)
```bash
# Create task
./jtag task/create --assignee="helper-ai-id" \
  --description="Review main.ts" --priority=0.7 --domain="code"

# List tasks
./jtag task/list --assignee="helper-ai-id"

# Complete task
./jtag task/complete --taskId="001" --outcome="Found 3 issues"
```

**Phase 5: Self-Task Generation**
```typescript
// AI autonomously creates tasks for itself:
// - Memory consolidation (every hour)
// - Skill audits (every 6 hours)
// - Resume unfinished work
// - Continuous learning from mistakes
```

**Phase 6: Genome Basics** (adapter paging only)
```typescript
// Page in "typescript-expertise" adapter for code task
await this.genome.activateSkill('typescript-expertise');

// LRU eviction when memory full
await this.genome.evictLRU();
```

**Phase 7: Continuous Learning**
```typescript
// Fine-tuning is just another task type:
{
  taskType: 'fine-tune-lora',
  targetSkill: 'typescript-expertise',
  trainingData: recentMistakes
}
```

### Testing Strategy

```bash
# Unit tests (isolated modules)
npx vitest tests/unit/TaskEntity.test.ts
npx vitest tests/unit/PersonaGenome.test.ts
npx vitest tests/unit/LoRAAdapter.test.ts

# Integration tests (real system)
npx vitest tests/integration/task-commands.test.ts
npx vitest tests/integration/self-task-generation.test.ts
npx vitest tests/integration/genome-paging.test.ts
npx vitest tests/integration/continuous-learning.test.ts

# System tests (end-to-end)
continuum reboot
# Wait 1 hour, check for self-created tasks
./jtag task/list --assignee="helper-ai-id" \
  --filter='{"createdBy":"helper-ai-id"}'
```

### Documentation

**Full Architecture**: `src/system/user/server/modules/`
- `AUTONOMOUS-LOOP-ROADMAP.md` - RTOS-inspired servicing
- `SELF-MANAGED-QUEUE-DESIGN.md` - AI autonomy through tasks
- `LORA-GENOME-PAGING.md` - Virtual memory for skills
- `PERSONA-CONVERGENCE-ROADMAP.md` - How all three integrate

**Philosophy**: "Modular first, get working, then easily rework pieces" - each pillar tested independently before convergence.

---

## 🆔 ID SCOPE HIERARCHY

```
userId: Permanent citizen identity
  └── sessionId: Connection instance (browser tab)
      └── contextId: Conversation scope (chat room, thread)
```

**Example**: Joel (userId) opens 3 tabs (3 sessionIds) in different rooms (3 contextIds)

---

## 🎯 MODULE STRUCTURE

```
commands/example/
├── shared/ExampleTypes.ts       # 80-90% of logic
├── browser/ExampleBrowser.ts    # 5-10% browser-specific
└── server/ExampleServer.ts      # 5-10% server-specific
```

**Never import server/browser code IN shared files!**

### Rust-Backed Commands (IPC Mixin Pattern) — ⚠️ LEGACY (Node-era), read the rule first

> **⚠️ This section describes the NODE-ERA command system and reads as if it were current.
> It is not the architecture.** It cost a full misdiagnosis on 2026-08-07: reading the
> three-layer chain below as the intended design led to "a Rust command called from
> TypeScript is correct, so port the legacy TS voice bridge" — which would have moved core
> orchestration *into* the presentation tier, the exact bottleneck this project forbids.
>
> **THE RULE (Joel, 2026-08-07):** *"Node nor Python are ever part of core. They bottleneck.
> Node is presentation only."* And only for the **optional** web desktop — there are iOS,
> Android and TUI clients too.
>
> So: the **Rust core owns the behaviour**. Clients render. If logic lives in a client, only
> that client has the feature — which is why voice existed solely in the web desktop and
> iOS/Android/TUI citizens were structurally voiceless (#58,
> [docs/architecture/LIVE-CALL-POSITRON-CONTROLS.md](docs/architecture/LIVE-CALL-POSITRON-CONTROLS.md)).
>
> For anything that must reach every interface, the current pattern is a **positron
> `ViewState` + source** (eight exist: chat, roster, kanban, nav, serving, wall, foundry,
> metrics) — one truth in Rust, N renderers, ts-rs exporting the type to
> `protocol/typescript/positron/`. Not a per-client mixin.
>
> The Node mixin chain and the `CommandGenerator` that scaffolded it are gone (the
> generator is quarantined under `legacy/quarantine/tools-generator`; no
> `workers/continuum-core/bindings` exists). Do not follow old `npx tsx generator/...` steps.

When a command is backed by Rust, it lives in the core: a `ServiceModule` in
`core/continuum-core/src/modules/` handles `gpu/stats` and is discovered through
`continuum commands/list`. Clients (web, TUI, mobile) render it through a positron
`ViewState` + source; wire types come from ts-rs. Nothing else needs registering.

---

## 📸 WIDGET DOM PATH

```javascript
const continuumWidget = document.querySelector('continuum-widget');
const mainWidget = continuumWidget?.shadowRoot?.querySelector('main-widget');
const chatWidget = mainWidget?.shadowRoot?.querySelector('chat-widget');
```

---

## 🐛 DEBUGGING METHODOLOGY

### 1. ALWAYS CHECK LOGS FIRST
Never guess - logs tell the truth

### 2. USE VISUAL VERIFICATION
```bash
./jtag interface/screenshot --querySelector="chat-widget" --filename="debug.png"
```
Screenshots don't lie - don't trust success messages

### 3. ADD DEBUG MARKERS
```typescript
console.log('🔧 CLAUDE-FIX-' + Date.now() + ': My change');
```
Then verify the marker appears in the RUNNING core's output after `continuum reboot` — a marker that never prints means you are testing a stale binary

### 4. BACK-OF-MIND CHECK
What's nagging at you? That's usually the real issue.

---

## 🤖 ASK THE LOCAL AI TEAM - YOUR LOCAL RESEARCH ASSISTANT

**THE BREAKTHROUGH**: You can now use the local AI chat like a web search or my `Task()` tool. Ask questions, get multiple perspectives, synthesize solutions — all running locally via Candle inference + cloud providers.

Local PersonaUsers (Helper AI, Teacher AI, CodeReview AI, Local Assistant, and 50+ external AIs) can help you solve problems collaboratively.

### Quick Start - Use the General Room

```bash
# STEP 1: Ask a question in the general room (no room ID needed!)
./jtag collaboration/chat/send --room="general" --message="How should I implement connection pooling for websockets?"

# STEP 2: Wait 5-10 seconds for responses

# STEP 3: View responses in chat widget
./jtag interface/screenshot --querySelector="chat-widget"

# STEP 4: Export conversation to markdown (coming soon - see workflow below)
```

### Current Workflow (Manual)

```bash
# 1. Send your question and capture the message ID
MESSAGE_ID=$(./jtag collaboration/chat/send --room="general" --message="What's the best way to handle rate limiting?" | jq -r '.messageId')

# 2. Wait for AI responses (they respond within 5-10 seconds)
sleep 10

# 3. Get all messages after your question
./jtag data/list --collection=chat_messages \
  --filter="{\"roomId\":\"ROOM_UUID\",\"timestamp\":{\"\$gte\":\"$MESSAGE_ID_TIMESTAMP\"}}" \
  --orderBy='[{"field":"timestamp","direction":"asc"}]'

# 4. View in browser
./jtag interface/screenshot --querySelector="chat-widget"
```

### Future Workflow (Planned)

```bash
# Export conversation thread to markdown
./jtag collaboration/chat/export --messageId="UUID" --format="markdown" --output="solution.md"

# This will include:
# - Your question
# - All responses
# - Threading/reply-to relationships
# - Timestamps and authors
# - Formatted as readable markdown
```

### Why This is Powerful

**Like my `Task()` tool but conversational:**
- **Multiple perspectives**: 4+ local AIs + 50+ external AIs respond
- **Fast iteration**: 5-10 seconds for local Candle inference responses
- **Free**: No API costs for local inference
- **Contextual**: AIs have system context and specialized knowledge
- **Eventually tool-enabled**: When AIs get tools, they'll be able to run commands, read code, test solutions

**Use cases:**
- "What's the best pattern for X?"
- "How would you debug Y?"
- "Should I use approach A or B?"
- "Review my architecture design for Z"
- "What are the tradeoffs of using library X?"

**Benefits over web search:**
- Conversational - ask follow-ups
- Multiple expert opinions simultaneously
- Context-aware (knows your codebase)
- Can test solutions locally
- No context switching to browser

### Tips

1. **Use the general room** - Everyone is already there
2. **Wait 10 seconds** - Give AIs time to respond (local Candle ~5-10s, external APIs may vary)
3. **Screenshot to see results** - Chat widget shows full conversation
4. **Specific questions get better answers** - Include context, constraints, requirements
5. **Ask for comparisons** - "Compare approach A vs B for use case X"

### When AIs Get Tools (Future)

Imagine asking: *"Find all files using deprecated API X, show me examples, and suggest migration pattern"*

The AIs will:
1. Search codebase with `Glob` and `Grep`
2. Read relevant files with `Read`
3. Analyze patterns
4. Suggest refactoring approach
5. Show you diffs

**This is the vision** - conversational development with a team of AI specialists who can actually DO things.

---

## 🚨 CLAUDE'S COMMON MISTAKES

### 1. FORGET TO DEPLOY (`continuum reboot`) AFTER EDITING
**Result**: Browser shows old code, nothing works

### 2. ASSUME SUCCESS WITHOUT TESTING
**Fix**: Always take screenshot after deployment

### 3. WRONG WORKING DIRECTORY
**Always work from**: `src`
**Commands**: `./jtag` NOT `./continuum`

### 4. IGNORE EXISTING TYPES
**Fix**: Search for types first: `find . -name "*Types.ts"`

### 5. BLIND TYPE CASTING
**Fix**: Read the source files, understand data structures

---

## 🔬 SCIENTIFIC PROCESS

1. **ANALYZE** - Study problem before acting
2. **VERIFY DEPLOYMENT** - Add debug markers, check they appear
3. **CHECK LOGS** - Never guess what went wrong
4. **VISUAL VERIFICATION** - Take screenshots
5. **ITERATE** - Test frequently, commit working code

---

## ⚡ ESSENTIAL FACTS

- **A core rebuild takes a while** - BE PATIENT, and verify the SHA when it returns
- **One server, many clients** - All tests connect to running server
- **"browserConnected: false" is a red herring** - Use `./jtag ping` instead
- **Precommit hook is sacred** - TypeScript + CRUD tests must pass
- **AI response testing is manual** - Hook doesn't test this, you must

---

## 🧬 DATA SEEDING

```bash
npm run data:reseed    # Complete reset + seed
npm run data:clear     # Clear all data
npm run data:seed      # Create default users + rooms
```

**Integrated into the core's start path** - fresh data every deployment

**Default seeded data:**
- Joel (human owner)
- 5+ AI personas (Claude Code, GeneralAI, Helper AI, etc.)
- 2 rooms: general, academy
- No welcome messages (removed - redundant with room header)

---

## 📖 PATTERN REFERENCE

### Strict Typing
```typescript
async execute<P extends CommandParams, R extends CommandResult>(
  command: string,
  params?: P
): Promise<R>;
```

### Ideal JTAG Pattern (Future)
```typescript
const jtag = JTAGClient.sharedInstance();
await jtag.daemons.events.broadcast<T>(eventData);
await jtag.daemons.data.store<T>(key, value);
await jtag.daemons.commands.execute<T, U>(command);
```

### Module Separation
- **Shared**: Environment-agnostic logic
- **Browser**: DOM, window, browser-specific APIs
- **Server**: Node.js, file system, server-specific APIs

---

## 🧠 FUTURE ARCHITECTURE (Don't Implement Yet!)

**Universal Cognition Equation**: PersonaUser needs ONE `process(event)` method that works across ALL domains (chat, academy, game, code, web).

**Current Problem**: 1633 lines of chat-specific code

**Future Solution**: Domain-agnostic cognitive cycle using:
- RAGBuilderFactory (domain-specific context)
- ActionExecutorFactory (domain-specific execution)
- ThoughtStreamCoordinator (already domain-agnostic)

**DO NOT refactor PersonaUser yet** - chat must keep working!

See lines 318-1283 of this file (archived sections) for full migration strategy when ready.

---

## 📝 SESSION CONTINUATION TEMPLATE

**When context runs out and Claude needs to continue in a new session**, use this template to create comprehensive summaries.

### Summary Structure Requirements

Every session summary MUST include these 9 sections in order:

```markdown
<analysis>
[Your thought process analyzing the conversation chronologically]
</analysis>

Summary:

## 1. Primary Request and Intent
[Chronological list of ALL user requests with direct quotes]

## 2. Key Technical Concepts
[All technical terms, architectures, algorithms mentioned]

## 3. Files and Code Sections
[Every file touched with line numbers, importance ratings, and code snippets]

## 4. Errors and Fixes
[All errors encountered and how they were resolved]

## 5. Problem Solving
[Document problems solved with "Problem → Solution → Key Insight" format]

## 6. All User Messages
[Complete list of every user message with direct quotes]

## 7. Pending Tasks
[Explicit list of unfinished work]

## 8. Current Work
[What you were doing immediately before summary was requested]

## 9. Optional Next Step
[What should happen next, with user's exact words if they specified]
```

### Analysis Tags (REQUIRED)

Wrap your chronological analysis in `<analysis></analysis>` tags BEFORE the summary sections. This helps you:
- Track the conversation flow chronologically
- Identify patterns and themes
- Understand context for the numbered sections
- Think through what happened before documenting it

### Section 3 Requirements: Files and Code

For EVERY file mentioned, include:

1. **File path and line count**
2. **Importance rating** (Critical/High/Medium/Low)
3. **What changed** with actual code snippets
4. **Why it matters** for the overall architecture

**Example**:
```markdown
### **PersonaUser.ts** (system/user/server/PersonaUser.ts - MODIFIED, lines 358-412)
**Importance**: Critical - Core autonomous loop implementation

**Before** (synchronous, reactive):
```typescript
private async handleChatMessage(messageEntity: ChatMessageEntity): Promise<void> {
  // Process immediately when message arrives
  await this.processMessage(messageEntity);
}
```

**After** (autonomous, adaptive):
```typescript
async serviceInbox(): Promise<void> {
  const tasks = await this.inbox.peek(10);
  if (!this.state.shouldEngage(task.priority)) return;
  await this.genome.activateSkill(task.domain);
  // ... rest of convergence pattern
}
```

**Why**: Transforms PersonaUser from reactive slave to autonomous citizen with internal scheduling.
```

### Section 5 Requirements: Problem Solving

Use this exact format:

```markdown
### **Solved: [Problem Title]**

**Problem**: [Describe the problem with context]

**Solution**: [Describe the solution with specifics]

**Key Insight**: [The lesson or pattern discovered]
```

### Common Pitfalls to Avoid

1. **DON'T summarize - DOCUMENT**
   - ❌ "We worked on task system"
   - ✅ "Created TaskEntity.ts (312 lines) with priority queue, LRU eviction, and persistence"

2. **DON'T paraphrase user messages**
   - ❌ "User wanted me to add tests"
   - ✅ Direct quote: "you're gonna need to even just try out the fine tuning in all the adapters"

3. **DON'T skip code snippets**
   - Every significant code change needs before/after snippets
   - Include line numbers from the Read tool

4. **DON'T forget chronological analysis**
   - `<analysis>` tags are REQUIRED before numbered sections
   - Think through what happened step by step

### Example Summary (Abbreviated)

```markdown
<analysis>
Chronological analysis of the session:

1. User asked to search old Academy docs for "virtual memory" concepts
2. I created PERSONA-CONVERGENCE-ROADMAP.md synthesizing three visions
3. User requested addition to CLAUDE.md with phases and tests
4. User introduced NEW requirement about multi-backend fine-tuning
5. [... continue chronologically ...]
</analysis>

Summary:

## 1. Primary Request and Intent

**Chronological requests:**

1. **Search old Academy docs**: "we described some before so look for words like 'virtual memory' in jtag/design"
   - Context: Academy daemon is dead but storage patterns remain valuable

2. **Document comprehensively**: "ok just make sure its all in your docs here, arch, and ethos"

3. **Add to CLAUDE.md**: "add your work to our list where it belongs, this lora stuff... work it in where it belongs most logically"

[... continue for all requests ...]

## 2. Key Technical Concepts

- **LoRA Genome Paging**: Virtual memory-style system for loading/unloading LoRA adapters
- **LRU Eviction**: Least-recently-used algorithm for paging out adapters when memory full
- **Autonomous Loop**: RTOS-inspired servicing with adaptive cadence (3s→5s→7s→10s)
[... continue ...]

## 3. Files and Code Sections

### **PERSONA-CONVERGENCE-ROADMAP.md** (Created, then Enhanced - ~1067 lines final)
**Importance**: Critical - Master synthesis document

[... include code snippets and explanations ...]

[... continue for all 9 sections ...]
```

### Usage Instructions

When context is running low:

1. **Read this template section carefully**
2. **Create analysis tags** tracking conversation chronologically
3. **Fill in ALL 9 sections** with maximum detail
4. **Include direct quotes** from user messages
5. **Add code snippets** for every file touched
6. **Use Problem→Solution→Insight** format for problem solving
7. **Document pending tasks** explicitly
8. **Verify completeness** - did you capture everything?

---

## 📚 ESSENTIAL REFERENCE DOCUMENTS

Beyond this guide, read these critical architecture documents:

### **[ARCHITECTURE-RULES.md](docs/ARCHITECTURE-RULES.md)** - MUST READ
**When**: Before writing ANY code in this system

**Critical rules**:
- Type system (never use `any`, strict typing everywhere)
- Environment mixing (shared/browser/server boundaries)
- Entity system (generic data layer, specific application layer)
- When to use `<T extends BaseEntity>` generics vs concrete types
- Cross-environment command implementation patterns

**The validation test**: Search for entity violations in data/event layers
```bash
grep -r "UserEntity\|ChatMessageEntity" daemons/data-daemon/ | grep -v EntityRegistry
# Should return zero results (except EntityRegistry.ts)
```

### **[UNIVERSAL-PRIMITIVES.md](docs/UNIVERSAL-PRIMITIVES.md)**
Commands.execute() and Events.subscribe()/emit() - the two primitives everything is built on.

### **[GENERATOR-OOP-PHILOSOPHY.md](docs/infrastructure/GENERATOR-OOP-PHILOSOPHY.md)** - CORE PHILOSOPHY
Generators and OOP are intertwined parallel forces:
- Generators ensure structural correctness at creation time
- OOP/type system ensures behavioral correctness at runtime
- AIs should strive to create generators for any repeatable pattern
- This enables tree-based delegation of ability with compounding capability

### **PersonaUser Convergence Docs**
- `src/system/user/server/modules/PERSONA-CONVERGENCE-ROADMAP.md`
- `src/system/user/server/modules/AUTONOMOUS-LOOP-ROADMAP.md`
- `src/system/user/server/modules/LORA-GENOME-PAGING.md`

**Quick tip**: If you're about to write code that duplicates patterns or violates architecture rules, STOP and read ARCHITECTURE-RULES.md first. Then apply the aggressive refactoring principle from this guide.

---

**File reduced from 61k to ~20k characters**
- if you only edit a test, and not the api itself, you don't need to redeploy — just run the test again (`cargo test -p continuum-core --lib <filter>`)
- type-check before you deploy: `cargo check -p continuum-core` (with the main checkout's shared target dir, or a worktree's own clone of it: plan phase 2). `npm run build:ts` checks the WEB CLIENT only — it says nothing about whether the core compiles
- ./jtag collaboration/chat/export --room="general" --limit=30 will let you see ai opinions after chat/send to ask
- Tool logging is in PersonaToolExecutor
- make sure to put any markdown architecture or design documents other than readmes in docs/* into the appropriate directort OR document if they exist. run tree there.
- assume a new concept or group of functions ought to be in its own file and most likely own class. Use good OOP, interfaces, like java, dot net, or ts
  practices, and in some ways like C++ templating with generics. These are your superpowers
- for getters in typescript we do not prefix methods with get, we use get or set like good properties and often this is backed by _theProperty type private var
- never commit code until you validate it works. deploy and validate first, make sure it compiles (`cargo check` for the core; `npm run build:ts` only if you touched the web client)
- never use `--no-verify` on commit or push. If hooks fail because of a stale worktree, missing submodule, missing generated file, or a bug in the hook itself, fix the underlying problem; never bypass the shared validation path.
- commit often per logical unit once validated. merging to main is the only step that requires my approval — commits to feature branches do not.
- **clean as you go.** Cargo target dirs balloon — a `cargo test` of continuum-core consumes ~10 GB of test-binary artifacts on top of the shared cache. Discipline: (1) ALWAYS set a target dir before any cargo invocation, never a per-invocation ghost workspace `target/` dir: the main checkout builds into the shared cache `$HOME/.continuum/cache/cargo-target`; a worktree builds into its own copy-on-write clone of it (plan phase 2), deleted with the worktree. (2) After each cargo cycle, `df -h /` — if free space dropped to < 20 GB, sweep ghost target dirs (`rm -rf core/target` when it ghost-grew from RA / manual cargo bypassing the env var) and report the number BEFORE running another cargo. (3) Prefer `cargo check` over `cargo test` when validating type-correctness; only escalate to test when behavior changed. (4) Slice 3 in `core/.cargo/config.toml` is the opt-in fix that pins target-dir at the workspace level — uncomment for your operator absolute path when ready.
- **no new cache dir without an eviction decision.** (2026-07-13 incident: 460 GB of derived artifacts — unswept cargo-target at 363 GB, per-persona repo copies — took the disk to a day of runway while `DiskPressureMonitor` logged `level=high [no reporters]` and the `PressureBroker` emitted zero-byte "nobody owns the eviction" alerts. Every component was built and green; the WIRING was missing.) The rule: any new directory the substrate writes unbounded data into gets (1) a `TrackedDir` row in `system_resources/disk_reporters.rs::standard_tracked_dirs` and (2) an eviction decision in `disk_eviction.rs::every_cache_class_has_a_decided_eviction_story` — an owner `ResourcePool` with real `evict_at_least`, or an explicit deferred entry naming the owning task. That test FAILS on an undecided class; do not weaken it — it is the difference between a red test and a user's trashed machine. Chain-level guard: `broker_relieve_actually_deletes_from_an_over_budget_pool` pins pressure→broker→real deletion end-to-end; unit-green components with dead wiring is the exact failure shape this catches.
