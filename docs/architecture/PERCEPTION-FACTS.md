# Perception Facts — typed provenance, one registry, A/B-ready

**Status:** design approved by Joel 2026-07-12 ("clean up the elegance and
modularity, adaptability of recent code in cognition"). Written after the
three-layer `[actions]`-suppression onion proved the string-typed approach
structurally unsafe.

## The problem (what 2026-07-12 proved)

Seven perception facts were built across three files in one sprint, each as an
inline string literal with its own ad-hoc trigger:

| Fact | Lives in | Trigger idiom |
|---|---|---|
| `[repetition]` (own) | deliberation_budget.rs | fn returning Option<String> |
| `[repetition]` (peer echo) | deliberation_budget.rs | fn returning Option<String> |
| `[actions]` zero-receipts | llm_deliberation_faculty.rs | inline `contains()` scan |
| `[context]` bounds | llm_deliberation_faculty.rs | inline, unconditional |
| `[unfulfilled]` | act_observe.rs settle arm | predicate + `record_action` |
| `[unverified]` | act_observe.rs settle arm | predicate + string scan of WM |
| `[confabulation]` | act_observe.rs settle arm | predicate + string scan of WM |

Working memory numbers EVERY entry as `[action #N]`, so the facts themselves
wore receipt numbering; recalled engrams carry receipts from earlier lives;
and every consumer re-derives semantics by parsing rendered strings
(`contains("[action #")`, `starts_with("[recall]")`). Result: the `[actions]`
fact — the cornerstone of the honesty stack — was silently suppressed all
afternoon by three compounding string-match bugs, each invisible until the
previous was fixed. The medicine suppressed the diagnosis. Type confusion,
not logic error.

## The design

**1. Typed working-memory entries (kill the string-scans).**
```rust
enum WmKind {
    Receipt { n: u32 },        // a real tool execution: "[action #3] I ran …"
    Fact(FactKind),            // proprioception: unfulfilled/unverified/confab
    Settlement,                // "I answered this" boundary marker
    Recalled,                  // surfaced from engrams — another life's data
}
struct WmEntry { kind: WmKind, text: String }
```
Render derives markers FROM the kind (`[action #3]` only for `Receipt`;
facts render `[unfulfilled]`/… with no receipt number). Every consumer that
today string-scans becomes a kind query: `entries.iter().any(|e|
matches!(e.kind, WmKind::Receipt{..}))`. `has_real_action_receipt` and the
`[recall]` prefix filter are DELETED, not maintained.

**2. One PerceptionFact registry (compression + adaptability).**
```rust
trait PerceptionFact: Send + Sync {
    fn id(&self) -> &'static str;              // "repetition-own", "actions-zero", …
    fn applies(&self, ctx: &FactContext<'_>) -> Option<String>;
}
fn perception_facts() -> &'static [&'static dyn PerceptionFact];
```
`FactContext` carries what the seven need today: `&Workspace`, the spoken
ring, typed WM view, visible-turn count. The faculty renders facts in ONE
loop (same pattern as `tool_call_formats()` — the registry idiom already
proven in this codebase). Adding fact #8 = one impl + one registry line.

**3. Per-fact toggles = A/B arms for free.**
Registry consults a `FactPolicy` (default: all on) keyed by fact id —
[[cognition-theories-get-ab-tested-personas-self-improve]] wants every brick
to be a toggleable hypothesis with a ledger row; this makes the toggle
structural. Snapshot-eval can run fact-on/fact-off arms without code edits.

**4. Probes per fact.** The registry loop emits one `probe!(class =
"perception.fact", id, fired)` — the glass box shows which facts rendered in
which prompts without capture spelunking (the 16:50 lesson: verify the
RENDERED prompt, never assume the code renders).

## Sequencing

1. `WmKind` + typed `record_*` verbs on WorkingMemory (`record_receipt`,
   `record_fact`, `record_settlement`); render from kind; migrate the three
   act_observe call sites; delete the string discriminators.
2. `perception_facts.rs`: move the seven facts; faculty renders via the loop;
   deliberation_budget keeps the pure geometry helpers (jaccard, rings) the
   facts call.
3. FactPolicy toggle + probes.
4. Tests move with their facts; the live specimens stay pinned.

## Non-goals

- No new fact semantics in this refactor — behavior-preserving move.
- The act→observe driver, decisions, and parser formats are untouched.
- The [Acting] block / tool-menu PX (menu shows one working example per fence
  idiom) is a SEPARATE brick, queued after this lands.
