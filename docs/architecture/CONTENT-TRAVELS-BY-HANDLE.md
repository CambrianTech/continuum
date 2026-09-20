# Content travels by HANDLE, never by copy

**Status:** design. The PRINCIPLE holds and is already implemented — by `spill` +
`tool/output`, which predates this document.

> ## ⚠ READ THIS BEFORE BUILDING ANYTHING BELOW
>
> **The mechanism already exists. Do not build a second one — I did, on 2026-08-18, and
> it was deleted the same day.**
>
> | want | use | where |
> |---|---|---|
> | park an oversized tool result and hand back a reference | `spill::spill()` | `cognition/tool_executor/spill.rs` |
> | let a citizen page / grep it | **`tool/output`** | `commands/tool/output.rs` |
>
> `spill` is content-addressed, per-persona-scoped *by directory layout* (hex-only handle
> = path-traversal guard), registered with the `PressureBroker` for eviction, and its
> verb does grep-with-context plus prebuilt `errors`/`warnings`/`failures`/`summary`
> filters so a citizen can find a build failure without knowing regex. It is better than
> what this document originally proposed.
>
> **What I built and deleted:** a `ContentSource` trait + `ContentRegistry` +
> `content/fetch` verb (723 lines). It duplicated spill with fewer features, had ZERO
> producers — so no citizen could ever obtain a handle for it — and still sat on the
> command surface as an AiSafe verb. A registered verb that cannot work is a lying
> affordance (#151/#357 class), and two verbs for one concept is the parallel-allocator
> sin (#8). Removed in full: module, registry, verb, ts-rs bindings, registration.
>
> **Why I missed it:** I searched for the CONCEPT in my head (`ContentSource`, handle,
> registry). The real thing is named for its JOB (`spill`, `tool/output`) — which is what
> a good name looks like, and exactly what my search could not find. See
> [[read-the-code-you-intend-to-replace-before-designing-its-replacement]].
>
> **What survives from the interface sketch below:** it is the right SHAPE for spill to
> grow into if `tool/output` ever needs a second implementation (a RAG source, a positron
> ViewState, a peer's artifact). Kept here as prose, deliberately not as code — an
> unimplemented trait with no consumers is itself a confusing leftover.

---

## The incident that forced it

A citizen mid-SWE-bench, 2026-08-18, was handed her own directory listing as:

```
[result #1840] bytes":22592},{"kind":"file","name":"header.py","path":"…
```

Head gone, opening mid-token, nothing saying anything had been removed. She could not tell
how many files existed, whether the call succeeded, or that she was reading a fragment.
Her prompt used 11,763 tokens of a 29,184 window at the time — the window was 60% empty.

The mechanism: `ToolResult.content` is a `String`. Every command produces typed output,
serde-serializes it, and drops it into that field. After that boundary the structure is
gone, so the only operation left on an oversized result is cutting characters — which two
different sites did with two different rules (one kept the head, one kept the tail).

## The three fixes that are WRONG, and why

Recorded because each one is locally the shortest path and each was attempted.

1. **Tune the cut point / keep the other half.** Both halves matter and which one matters
   depends on the payload: a listing identifies itself at the front, a test run reports at
   the back. Any fixed rule is wrong about half the time, silently.

2. **A smarter generic reducer** (shed array elements, shed lines, mark the elision).
   Better than cutting bytes, still wrong: a generic reducer cannot know that a diff is
   atomic, that a patch reduced by half applies cleanly and does the wrong thing, or that
   dropping lines from a file body shifts every line number the next `code/edit` depends
   on. *Half a code fix is worse than no fix: no fix fails loudly, half a fix passes
   review.* The distinction is not in the bytes — it is in what the payload MEANS.

3. **Raise the budget fraction.** Picking `1/8` instead of `1/32` is the same hardcoded
   guess with a different number, and it does not stop the corruption — it only makes it
   rarer and therefore harder to find.

All three share one defect: **the substrate inspecting a payload it does not understand,
and deciding.**

## The rule

> **Content never travels. A reference to its source does.**

A tool result is a small **header** plus a **handle**. The header always fits and is always
complete — what this is, how large, how to address it. The full content stays where it was
produced. A mind that wants more calls the handle.

This is not a new mechanism. It is the one the RAG and positron layers already use: one
truth at the source, N projections, the consumer pages. The tool-result channel must use
*that* interface rather than growing a parallel one.

## The interface

Polymorphism, not inspection — the OpenCV-style `cv::Algorithm` shape this codebase
already prescribes. Nobody switches on what the content is; they hold a reference and call
its method, and the implementation decides.

```rust
/// A thing that produced content and can still be asked about it.
/// Implemented BY the producer — a file engine, a listing, a RAG source, a positron
/// ViewState, a peer's artifact on another node. Callers never know which.
pub trait ContentSource: Send + Sync {
    /// Small, complete, always fits. Identity + extent + how to address it.
    fn header(&self) -> ContentHeader;

    /// Dereference. The SOURCE decides what a range means for its own content —
    /// lines for a file, entries for a listing, hunks for a diff, a page for a board.
    fn fetch(&self, range: Range) -> Result<Slice, SourceError>;
}
```

Consequences that fall out for free, none of them special-cased:

- **Nothing is ever malformed.** The source hands out its own content in its own units.
  A listing yields whole entries; a file yields whole lines with their true numbers.
- **The window stops being the constraint on truth.** A 40k-line file is fully available
  to a citizen on a 4k window — she pages. Content size and context size decouple.
- **Indivisible payloads refuse, and say how to narrow.** A diff's `fetch` declines a
  partial range because *its implementation* knows that; no central policy table.
- **It works across the grid unchanged.** A handle to a peer's artifact is the same
  interface as a local one — which is why this belongs to the substrate, not to cognition.
- **Receipts become retrievable.** The act-history gap ([[act-results-need-a-recency-channel-not-semantic-recall]],
  tasks #390/#414 — 2,863 acts executed, one visible, no path to the rest) is a handle
  problem: the results still exist, nothing hands her a reference to them.

## Reduction, if it happens at all, is the producer's

Reduction is the fallback, not the design. Where a header genuinely must summarize, the
*producer* summarizes, because only it can do so truthfully. The substrate's contribution
is to never invent one.

The better move is upstream of that: the caller **asks for less**. `code/read` takes a
line range, `code/list` a path, `code/search` a pattern. Planning the ask in advance beats
repairing the answer afterwards — and a handle makes asking again cheap.

## The positron convergence — two ways to the same truth

`ContentSource` and a positron `ViewState` are the same shape wearing different names:
one truth at the source, N projections, the consumer reading at its own rate. They must
not stay two mechanisms.

What that gives a citizen is a choice she makes for herself, per situation:

- **Observe the layer.** The positronic projection is the ambient, condensed view — the
  board, the room, the run. She sees the shape of things without asking for anything, the
  same way a person glances at a screen.
- **Reach for the handle.** When something in that view warrants it, she dereferences and
  reads the detail at whatever depth the question needs.

That is how a person actually works: you do not read every line of every file in a repo,
you carry a summary and drill in when something looks wrong. The condensing is not a
budget mechanism we impose — it falls out of the citizen choosing her own resolution, and
it is *correct* rather than lossy, because the detail is always one call away and she knows
it. Minutia is skipped, never destroyed.

This also settles what "too big to send" means. Nothing is too big. There is a view, and
there is a way in. The window sizes her ATTENTION, not the truth available to her.

Practically: a positron projection should be able to hand out handles into what it
summarizes (a board row → the card's full content; a run tile → the transcript), and a
`ContentSource` header is already the smallest possible projection. Converging them is the
next design step after the build order below — the same `(header, handle)` pair, whether
it arrives through a tool result or a rendered view.

## Collapse-and-expand is the universal idiom — and it is positron's actual purpose

The mechanism above is not novel and should not be. **Collapse a tree, expand what you
care about** is how every IDE, every file browser, every code-review tool and every
coworking surface has worked for decades, because it is how attention actually works. We
are not inventing an interaction; we are giving citizens the one humans already have.

What makes it load-bearing here is that the SAME structure serves both audiences:

| | human | citizen |
|---|---|---|
| collapsed | a folded section, a card, a row | a header line in the prompt |
| expand | click | `content/fetch` |
| budget | screen space + attention | context window + attention |
| what's underneath | identical | identical |

A prompt IS a rendering. It has a viewport, a scroll budget, and an expansion gesture —
they are just spelled differently. Which is why this belongs to positron and not to
cognition: **positron's real job is to bridge UX and PX**, one definition projected to N
renderers, and the persona's prompt is simply one of the renderers. Everything we build
for the human screen should light up for a citizen for free, and the reverse. Anything
that only reaches one of them is a projection we failed to define once.

That is the Flutter analogy, and it is the right one: you author the surface generically,
and every target renders it natively. Here the targets are a browser, a phone, a TUI —
**and a mind**. A citizen of any kind, on any model, gets the same capacity to navigate
what exists, because the capacity lives in the projection rather than in the client.

### The cycle this closes

Collapse/expand is not a one-shot. It is the loop:

1. She perceives a condensed view — headers, summaries, the shape of things.
2. Something warrants attention. She expands it (a tool call, a handle).
3. **Her thinking section and her tool receipts carry the handles forward**, so what she
   opened stays reachable next turn instead of aging into nothing.
4. The next perception is condensed again, but around what she now knows.

That is a cyclical RAG consciousness rather than a one-shot retrieval: attention narrows,
detail arrives, the frame re-condenses around it, and the cycle repeats. The handle is
what makes step 3 possible — a receipt that carries a reference stays *live*, where a
receipt carrying a truncated copy is dead the moment it ages out. (This is the same gap
#390/#414 measured from the other side: thousands of acts executed, one visible, no path
to the rest.)

### The single-prompt constraint is current, not intended

Today all of this has to fit in one prompt, because that is what these LLMs accept. That
is a property of the serving interface we intend to fix, NOT a property of the design — so
nothing here should encode "one prompt" as an assumption. Handles are what make the
eventual fix cheap: when a mind can hold a persistent, incrementally-updated working set
instead of being re-rendered whole each turn, the projection layer does not change at all.
Only the renderer does.

Which is the test for any future work in this area: if it would have to be redesigned when
the one-prompt constraint lifts, it is encoding the constraint instead of the intent.

## Build order — REVISED after the duplicate was deleted

1. ~~Reconcile the two handle models (#17)~~ — moot for now. `spill` uses a
   content-addressed hex stem; nothing else competes with it, because the competitor was
   deleted. #17 only becomes live again if a SECOND `ContentSource` implementation
   appears and needs to share a handle type with spill.
2. ~~`ContentSource` in the substrate~~ — **do not build this speculatively.** Grow it out
   of `spill` at the moment a second implementation actually exists, so the trait is
   extracted from two real cases rather than imagined from zero (CLAUDE.md's outlier
   rule, applied honestly).
3. **The remaining real defect:** `working_memory`'s `recent_results_chars` re-cuts a
   result that the executor ALREADY bounded and handle-backed, severing the
   "your full output is saved as `<handle>`, page it with `tool/output`" sentence the
   executor wrote. The citizen is told how to recover her output and the budget layer
   cuts the telling off. Fixing it properly means `fold_with_recovery` returning
   `{preview, Option<SpillRef>}` instead of a String with the handle baked into prose,
   so the handle travels as DATA to working memory and the render layer cannot cut it.
4. **Then** the recency fold and recent-results tail-keep can go, because a handle-backed
   result has nothing left worth re-cutting.

## Forbidden moves

- Any code path that inspects a payload's bytes to decide how to shrink it.
- Any new size fraction expressed as a bare denominator over the window.
- Any reduction performed by a consumer rather than the producer.
- A second handle type. There is one, and #17 is where it gets settled.
