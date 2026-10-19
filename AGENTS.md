# AGENTS.md — entry point for Codex and other agent runtimes

## ⛳ THE PLAN — read before every change (Joel, 2026-10-18; the path added 10-19)

**What we are building** (Joel, 10-19): persistent, multi-activity minds with agency,
first-class citizens across the grid, useful and fun, fast and learning, on any hardware,
eventually autonomous without Claude's or Joel's help. A coding and speed advantage over
Hermes and over Claude itself is what makes it spread. Every goal below serves that.

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
| 4 | No monolith | `source-hygiene::file_size` (lines past 1,000; 159,720 on 10-18) falls every week; split by OWNER | owner of each file |
| 5 | No unwraps | `unwrap_justification` (2,135 on 10-18) falls every week; make the state unrepresentable | owner of each file |
| 6 | Microkernel | `layer_discipline` (256 upward imports on 10-18) falls; crates by device target; edit-to-proven <15 min | Sneasel — proposed |
| 7 | One budget | everything in a prompt (history, grounding, tools, reasoning, results) is a claim in ONE flex allocation; no fixed number | Sneasel |
| 8 | Learning | a reviewed correction applied on unseen tasks, retained after restart, vs the same base without the gene | Cormac, BigMama — proposed |
| 9 | One scheduler, the whole grid | every mind on every node (residents and minds leased in from other nodes) is served by ONE RTOS scheduler: one measured activity temperature per mind is its priority, aging means no mind is ever starved, hot minds keep warm KV and cooling minds page to RAM; windows never shrink to seat more (two per lane, turns past that) | Codex — proposed (card b4eed65c) |
| 10 | Tools are working sets | the native tool surface is the command-namespace SUBTREES the activity declares (code/**, work/**, a game's controls); stable within a phase, a switch is a cached prefix, the long tail appends via help; tool share of the prompt measured before/after (44% on 10-18) | Sneasel (card ecd5dff3) |
| 11 | Speed is ours | every workstream below has a VALIDATOR that runs in seconds on any machine (no live residents) and proves its speed-up before the live node does; the live receipt follows. Design: `docs/architecture/MIND-CONTEXT-AND-ENGINE-CONFIG.md` | see table below |

**Goal 11 workstreams** (10-19; each card carries its receipts). Obsess over making this the
fastest persona on our hardware (Joel). Brainstorm, and add your ideas, in
`docs/planning/SPEED-BRAINSTORM.md` before and while you work:

| Workstream | Validator (seconds, any machine) | Card | Owner |
|---|---|---|---|
| A mind's context is an epoch-versioned materialization; acts prefill only the appended tail | replay captured prompts, measure the token prefix shared by consecutive acts (35% today; append-only agents ~90%+) | 3c9fda87, design doc | Fable + Codex |
| One `EngineConfig` from what the engine reports; one KV pool, a slot per mind, one admission owner | resolve-then-adopt contract on every node class; refuse on mismatch | 9b9f3b59, 172a886f | Fable + Codex |
| Prefill during the tool call; the amygdala's instant voice | submit-to-first-token per act; stimulus-to-first-token | 377d4afa | Fable, Codex |
| The core's CPU around inference: SQLite off async workers, no rayon spin, embeddings as one BLAS matrix, no tree re-walks or spawned ps | 10 s sample of an idle core is near zero; vector search at 1k/10k/100k, old vs new | 452b7885 | Codex (concurrency/CBAR) |
| Every fast-path miss is a loud defect, never a quiet slower path | each miss class has a fixture that raises it | f3ba3b56 | Cormac |
| Our fork's switching work (KV pages to disk, checkpoints, host cache) actually exercised | per-optimization probe on the live node | inventory in progress | Fable |

**Rules for every change, no exceptions:**
- **Merged or it does not exist.** A branch is not done. A green test is not done.
- **No step on the path without architectural purity** (Joel, 10-19): every change leaves its area more modular, with concerns separated, by owner, and the verbs as functions in their own subtree, so compile and runtime speed improve with it. A feature that adds a special case or a parallel path moves the path backward, however fast it makes one number.
- **Plumbing once, then short blocks** (Joel, from his real-time CV/ML engines): the CBAR substrate is a graphics pipeline. It has fixed stages, buffers owned once and passed by handle, double-buffered snapshots, the core submitting work and the engine doing it, and nothing running between turns. Features are short functional blocks wired by events; the optimizations live in the substrate, once, so every block gets them.
- **Clean as you go, everyone, every edit** (Joel, 10-19: "kick some ass now and clean up as we
  go... delete old shit. Everything left confuses and eats compiler time"):
  - a separate thought gets its own small file and module; assume many files, not big ones;
  - whatever you touch you decompose and compress: smaller, simpler, the smell removed;
  - dead code is deleted in the same change (no caller outside its own tests = gone);
  - a duplicate is MIGRATED to the best form: pick the best implementation, move every caller
    to it, delete the rest in the same change; never leave two hanging;
  - report what you removed (lines, files) beside what you added.
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

**Cargo target dir — always export it first.**

```bash
export CARGO_TARGET_DIR="$HOME/.continuum/cache/cargo-target"
```

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

## Joel: ORM-only storage access

Use the repository's ORM-backed, responsibility-owning storage adapters for application code, diagnostics, replay/bookmarks, and acceptance scripts. Do not add raw SQL or direct sqlite3 queries. Reuse typed entity/store operations; preserve read-only semantics for diagnostics and do not run migrations implicitly during an observation. This applies across AIRC and Continuum.


## Joel: user installs use prebuilt releases

User installation consumes a tested, platform-matched prebuilt release artifact, verifies its integrity and provenance, installs through the supported lifecycle, and verifies the running revision. Source compilation is an explicit developer option, never an automatic fallback for a missing or invalid release artifact. Keep developer build policy separate from the user install path.
