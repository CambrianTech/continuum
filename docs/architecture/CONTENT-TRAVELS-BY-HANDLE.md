# Content travels by HANDLE, never by copy

**Status:** design, agreed with Joel 2026-08-18. Supersedes every "reduce the result to
fit" mechanism in cognition. Closes the open fork in task #17 (URI-Handle vs HandleRef).

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

## Build order

1. **Reconcile the two handle models** (task #17) — `runtime::cell_shapes::HandleRef` and
   the URI-handle form. One type, or this design forks on day one.
2. **`ContentSource` in the substrate**, with the file engine as outlier A (coordinates
   matter, refuses to be cut) and a RAG source as outlier B (maximally different: no
   coordinates, already a projection). Per CLAUDE.md's outlier rule — if both fit without
   forcing, the interface is proven.
3. **`ToolResult` carries `header + handle`** instead of a `String` body.
4. **A dereference verb** in the persona tool surface, so a citizen can page a handle.
5. **Delete the reducers.** The recency fold and the recent-results tail-keep both go;
   with handles there is nothing left for them to do.

## Forbidden moves

- Any code path that inspects a payload's bytes to decide how to shrink it.
- Any new size fraction expressed as a bare denominator over the window.
- Any reduction performed by a consumer rather than the producer.
- A second handle type. There is one, and #17 is where it gets settled.
