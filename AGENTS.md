# AGENTS.md — entry point for Codex and other agent runtimes

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
