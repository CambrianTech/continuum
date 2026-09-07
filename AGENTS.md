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

## Merging

Commits to feature branches and merges to `canary` do not need the repo owner's
approval; **merging to `main` does**. Validate before you commit — deploy and exercise
the change through a command, and read the receipt rather than the exit code.
