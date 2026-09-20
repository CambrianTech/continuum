# REALLY GOOD HINTS — the focus layer between RAG and the LLM

> "All of a sudden it's an opencode LLM with REALLY good hints."
> "It'll be like a dumb looper in a lot of ways, in terms of speed, like hermes or
> opencode when doing code — and what's really going on is these other processes
> are working really hard, just not synchronously or latched, streamlining the RAG
> for the given ask with contextual knowledge of what is happening across the board."
> "These low-latency async processes are where we shine."

## The thesis (and the inversion)

The naive read of "make the hot path fast" is "make it dumb": strip context,
shrink the prompt, accept worse decisions for lower latency. **That trade is
false here.** A focused LLM handed a tight, *relevant* context decides BETTER
than the same model drowning in a 16k-token dump of every tool, every parameter,
and the full standing grounding. Focus buys speed AND intelligence at once,
because attention is the scarce resource — for the model exactly as for us.

So the shape we are building:

- **The hot path looks like opencode.** A fast act → observe → decide-next loop.
  From the outside it reads as a "dumb looper." That is fine — that is the point.
  One LLM call per tick on the critical path; everything else deferred.
- **The intelligence is asynchronous.** The faculties (recall, grounding,
  world-model, tool-surface knowledge, situational awareness) run *continuously
  and off the critical path*, RANSAC-style: always refining a best-current answer,
  never stop-the-world. They are "working really hard, just not synchronously or
  latched."
- **A focus layer turns their work into REALLY GOOD HINTS.** Between the full RAG
  and the LLM sits a consolidation stage that streamlines the context *for the
  given ask*, with contextual knowledge of what is happening across the board.
  Post-tool-run? The persona doesn't need the standing grounding re-dumped — it
  needs the tool result + the affordances for "what next." New question from a
  teammate? Fuller grounding. The focus layer decides.

The only intrinsic per-turn deadline remains GPU LLM inference. Recall is already
deferred; the four grounding sources are next. The focus layer is what makes the
deferral *pay off as quality*, not just as latency.

## Where it lives — the Arbiter is the focus seat

The architecture already has the seam. `cognition/workspace.rs`:

```rust
pub trait Arbiter: Send + Sync {
    fn select(&self, candidates: Vec<Contribution>, capacity: usize) -> Vec<Contribution>;
}
```

`SalienceArbiter` (the bootstrap) is a dumb top-k by ML salience: greedy, no idea
what the *ask* is. It is compression #1 (truncate to capacity) but it is blind to
the situation. **The focus layer is a richer `Arbiter`** — situation-aware,
consolidating rather than merely truncating.

That requires evolving the seam to carry the situation:

```rust
/// What the focuser knows about THIS tick beyond the raw bids — the ask, and
/// what's happening across the board, so it can streamline the RAG for the
/// given ask instead of dumping everything.
pub struct FocusContext<'a> {
    /// The consolidated burst / ask this tick (the world-state).
    pub world_state: &'a str,
    /// The situation: was the last externalized act a tool run (→ minimal
    /// context, just the result + next-step affordances) vs a fresh question
    /// (→ fuller grounding)? Carried as a typed signal, never inferred by
    /// reading the model's words back.
    pub situation: Situation,
    // (future) live snapshots of the async lanes' best-current state.
}

pub trait Arbiter: Send + Sync {
    fn focus(&self, candidates: Vec<Contribution>, capacity: usize, ctx: &FocusContext)
        -> Vec<Contribution>;
}
```

Three implementations, validated by the outlier doctrine (build the simplest and
the most different; the middle is then guaranteed):

1. **`SalienceArbiter`** — the bootstrap (outlier A). Keep it; it is the floor.
2. **`FocusArbiter`** — algorithmic, situation-aware (outlier B, maximally
   different): post-tool-run drops re-grounding and keeps the tool result +
   affordances; fresh-ask keeps fuller grounding; dedups overlapping bids;
   respects a token budget, not just a count. Pure Rust, fast, deterministic,
   glass-boxable.
3. **`LlmFocusArbiter`** — the endgame: a small, fast model consolidates the
   full context into a tight brief (the "really good hint"). Slots into the same
   seam. Built only once the algorithmic version proves the interface and the
   glass box shows where a learned focuser would beat it.

"Do we add intelligence or algorithms here?" — **both, behind one trait, in that
order.** Algorithm first (cheap, deterministic, measurable), learned focuser when
the scoreboard says it earns its latency.

## The guardrail: focus the INPUT, never filter the OUTPUT

This is attention, and it is legitimate precisely because it operates on the
*input* context — what the model gets to attend to — not on the model's output.
The forbidden move ([[no-hardcoded-heuristics-to-steer-cognition]]) is a function
that reads the persona's *generated words* and puppets/overrides them (phrase
blocklists, etc.). The focus layer never touches the output; it curates the
context the model reasons over. That is what every brain does — it is the
opposite of puppeteering.

The starting `FocusArbiter` heuristics (post-tool-run → minimal) are
attention-routing rules on the *situation*, not output filters. And like the
arbiter itself, the focus policy is a documented seam meant to become *learned*
(the `LlmFocusArbiter`, and ultimately a genome-trained focuser) — the algorithm
is the bootstrap, not the destination.

## First concrete win: the tool surface (16k → relevant + progressive disclosure)

> "Lots of compression to do in RAG in a real-time way — from 16k of listing
> every god-damn tool and parameter to just the tools we might be interested in
> and a way to get help for them. What is one more step? It might learn to know
> the tools anyway. Make it like a good UX for prompts."

The tool catalog (`cognition/persona_tools.rs` `authorized_tool_specs`) is the
single biggest compressible block in the deliberation prompt. Treat it as a
budgeted RAG source under the focus layer:

- **Surface only candidate-relevant tools** for the current ask (relevance-ranked
  against the situation), not the full authorized set.
- **Progressive disclosure** — "what is one more step?": offer a `tool/help` (or
  `describe`) affordance the persona calls to expand a tool's full parameter
  schema *on demand*, instead of carrying every parameter for every tool upfront.
  Minimal upfront, drill-down available. Good UX for prompts.
- **It learns the tools anyway** — over turns (and ultimately via the genome
  loop) the model internalizes the common surface, so the upfront list can shrink
  further. The hint gets cheaper as competence grows.

This is the same continuous-budgeted-compression pattern (deferred lane + focus)
applied to the tool surface. It connects to the command-registry consolidation:
once every command is on the one `DynCommand` registry, the *authorized* set is
large and correct — which makes the focus layer's job (show the relevant few)
load-bearing, not optional.

## The dashboard (not an archeological dig)

> "It's more like a dashboard than an archeological dig."

Introspection has two faces behind ONE seam — `WorkspaceCaptureSink`:

- **The dig (exists):** `JsonlWorkspaceCaptureSink` appends one line per tick for
  replay, regression, and training corpora. Forensic, read-after.
- **The dashboard (to build):** a `watch`-publishing sink that surfaces the
  *current* tick live — per-faculty timings, the assembled (focused) context with
  its token budget, the decision, and each async lane's best-current state /
  staleness / hit-rate. Real-time operational view of the mind working.

The per-faculty live timing (landed: `FacultyTiming` on `WorkspaceTrace`,
schema v2) is the dashboard's first feed and the scoreboard's speed axis. After
the focus layer + deferral, the dashboard should visibly show the perception tier
at ~0µs and the LLM dominating — and the prompt token-budget panel should show
the tool block collapse from 16k to the relevant few. That visible collapse,
with decision quality holding or rising on the gym grade, is the proof that
"focused beats verbose."

## Build order

1. **[done] Live per-faculty timing** → glass box / dashboard speed axis
   (`FacultyTiming`, schema v2).
2. **Dashboard sink** — `watch`-publishing `WorkspaceCaptureSink`; live view over
   the timing + focused context + token budget.
3. **`FocusContext` + `Arbiter::focus`** — evolve the seam to carry the situation;
   port `SalienceArbiter` (outlier A).
4. **`FocusArbiter`** (outlier B) — situation-aware algorithmic consolidation
   (post-tool-run minimal; budgeted; dedup). Validate the interface across the two
   outliers.
5. **Tool-surface focus** — relevance-rank the tool catalog + `tool/help`
   progressive disclosure; measure the 16k→Nk collapse on the dashboard with
   decision quality held on the gym.
6. **Defer the 4 grounding sources** (roster, doctrine, active-work,
   workspace-map) in `DeferredFaculty` — the async hint-makers feeding the focus
   layer; budgeted, introspectable.
7. **`LlmFocusArbiter`** — learned focuser, only once the scoreboard says it earns
   its latency.

Each slice is measured on the four-axis scoreboard (speed via `TurnMetrics` +
`FacultyTiming`; quality/lift via the gym grade) — "measurably improving," not
hoped-for.
