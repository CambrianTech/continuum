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
