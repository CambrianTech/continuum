# Boot Is a Typed Plan, Not a Script

*2026-09-02. Joel, after the boot audit: "Clearly so many unreliable disjoint steps
still." Correct — the audit fixed instances (the lane murder, the front-loaded desktop
build); this doc fixes the constraint.*

## The disease

`tools/scripts/start-server.sh` is ~920 lines of accreted rails: foreign-server reap,
lane adoption, airc daemon, three cargo sidecar builds, a bridge staleness check, an
eye-node spawn, an STT model fetch, a web build, freshness guards — in bash, ordered by
accident of when each was added, with no shared notion of dependency, timing, retry, or
receipt. Every incident this week was a seam between two rows that couldn't see each
other (the stop path murdering the lane the start path adopts; the desktop build gating
a core it merely depends on). A bag of steps is reliable only by random chance — each
new rail multiplies the seams.

## The law

**Boot is a typed, ordered, receipted plan executed by the Rust CLI — the same
ServiceModule discipline the runtime already applies to everything after boot, applied
to boot itself.**

```rust
struct BootStep {
    name: &'static str,
    /// Steps this one needs completed first — an explicit DAG, not file order.
    needs: &'static [&'static str],
    /// Optional steps skip on failure with a receipt; required steps abort loudly.
    required: bool,
    /// Where it runs relative to the core: Before (rare — things the core dials
    /// at init), Beside (spawned async, core does NOT wait), After (needs the
    /// core answering).
    phase: Phase,
    // run() does the work; verify() proves it took (the #194 discipline per step:
    // a step that cannot prove itself did not happen).
}
```

The executor walks the DAG, runs `Beside`/`After` steps concurrently where the DAG
allows, stamps every step with `{name, outcome, ms}` into ONE boot receipt (probe class
`boot.step`), and prints the plan it is about to run before running it — so "what does
boot do" is answered by the system, not by reading bash. `start-server.sh` shrinks to:
resolve the CLI, `exec continuum boot`.

What this buys, concretely:
- **The seams become edges.** "desktop needs core" and "adoption needs stop-spared-the-
  lane" are DAG edges the compiler and the receipt both see — the lane-murder class
  becomes unrepresentable rather than re-fixable.
- **One receipt.** Every boot produces a timing table; a slow boot names its row; a
  regression is a diff between two receipts, not a feeling.
- **Optional means optional.** Desktop, bridge, eye-node, moonshine are `Beside`
  optional steps — a fresh clone's core answers in seconds and the extras land behind
  it, receipted.
- **The self-proof battery slots in.** `voice/selftest` and siblings are `After` steps —
  boot ends with the system having proven its own senses.

## The lifecycle taxonomy (Joel 2026-09-02: "shut down, reboot, or shutdown-light save and restore")

Three verbs, each a typed answer to *what survives the seam* — never ad-hoc flags:

| Verb | Live lane | KV slots | Durable state (rounds, memories, rooms) |
|---|---|---|---|
| `shutdown` | dies | discarded | files remain; nothing running |
| `reboot` | dies (SAVED first) | **saved to disk fast** (`--slot-save-path`; restore measured near-instant), restored by the next start | resumes all — rooms pick up where they were |
| `shutdown-light` | dies (saved first) | saved + restored, same as reboot | resumes all |
| `pause` / laptop-lid (OS sleep) | process PAUSED in place (not killed) | held in RAM; re-verified on wake | resumes all — the wall-clock gap is the only change |

**Pause/resume is the felt model (Joel 2026-09-02: "shut my laptop or shut
down ought to work like pause/resume, every system smart about fast
restore").** From the citizens' side, laptop-lid and shutdown are the SAME
experience — freeze, then wake exactly where we were. They differ only in
whether the process survives: OS sleep pauses it in place (state already in
RAM, but the wall-clock jumped, so on wake every time-based subsystem must
RE-VERIFY rather than assume continuity — a lane health-check, a served-window
re-probe, a round idle recompute against the real now, NOT the pre-sleep now);
shutdown/`shutdown-light` save-and-die, and the next start restores. The
save/load broadcast (`ServiceModule::save_state`/`load_state`, #3447) is the
one mechanism both use. The invariant: a citizen never perceives a seam as
loss — only as elapsed time ([[continuity-is-the-default-reset-is-the-exception]],
[[time-based-convergence-dies-under-restarts-and-per-tick-probes-rotate-truth-away]]).

**KV cache is just another save_state participant — that's what makes it
seconds, not minutes (Joel 2026-09-02: "even KV cache, so you don't miss a
beat… each concern loading state via our get-for-free base, like a daemon
calling each subscriber via its interface").** The warm KV of every serving
lane is state like any other: `ServingDaemonModule::save_state()` persists the
slots to disk (llama-server's `--slot-save-path`), `load_state()` restores
them — through the SAME `ServiceModule` trait, broadcast by the runtime to
every subscriber IN PARALLEL (#3447's bounded save-and-join, and its
`load_all_state` mate). Nobody writes bespoke KV-persistence plumbing: the
concern implements the two trait methods it already has for free, the daemon
iterates subscribers through the interface, and restart is warm — the citizen
resumes mid-thought, cache intact, not re-prefilling from zero. This is the
[[everything-pages-the-grid-is-one-more-tier]] principle at the lifecycle
seam: KV → disk → KV is one more page, and it rides the one lifecycle every
concern shares.

**NO PROCESS SURVIVES A SEAM** (Joel 2026-09-02: "Shut the mofos down — it's
not right to battle the existing system, other than having it save state and
shut down fast"). A first cut leaked the warm llama lane across the reboot
seam for the successor to adopt; that is the battling-the-existing shape —
two generations verifying each other's survivors — and it was reverted the
same hour. Speed comes from SAVE/RESTORE, never from inheritance: the old
system's whole job at a seam is save fast, die fast, completely. The
remaining latency work is therefore (a) fast state save on stop, (b)
measuring where serving-ready time actually goes on a clean start (the ~15
minutes is assumed to be model load and has never been decomposed).

## Migration (strangler, one row at a time — never a big-bang rewrite)

1. `continuum boot` lands with the executor + the three steps that caused this week's
   incidents: lane adopt-or-reap, core launch + SHA verify, desktop-beside. Script rows
   delete as their step lands — a row may not exist in both worlds.
2. Sidecar builds (mcp, cli, custodian, bridge-stale) as `Beside` steps with
   only-if-changed hashes.
3. airc daemon, eye-node, moonshine as `Beside` optional.
4. `reboot` = the same plan with `keep_lanes` — the early stop retires (the plan's
   stop step runs at its DAG position: after the new binary is built, before exec),
   converting build-time downtime into zero.
5. The script is 10 lines. Delete the rails vocabulary.

## The bar

*A fresh clone runs one command; the core answers within seconds of binary-ready; every
step of what happened is one receipt; and no step can silently kill what another step
depends on, because the dependency is typed.* That is the boot the demo needs and the
boot a stranger can trust.
