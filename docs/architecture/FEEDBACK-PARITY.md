# Feedback parity: a mind hears the world at least as well as Claude does

Joel, 2026-11-11: "You literally know what you do and how you get feedback (which we can do BETTER
if we try) so we know the gaps." The spec for a resident's feedback is the loop a frontier agent
(Claude Code) already runs on, in any activity, any language, any program. Every row is a channel;
the gap column links the card that closes it. "Verify" means nobody has measured it yet: measure,
never assume.

| Channel (how Claude gets it) | A resident today (2026-11-11, M5 captures and source) | Gap / card |
|---|---|---|
| A command's whole outcome: stdout, stderr, exit status | Shell result; large output kept as an expandable handle | none |
| A long command returns a handle at once; ANY end of it (exit, crash, kill) wakes the agent | A completion push exists (`ExecutionState.handed_back` -> `command:completed`); Benchy read a running build 4 times instead; a core restart lost her command with `not_found` | ce191b8c |
| Any stream as events (a growing log, a process it did not start, a message bus), each new line delivered, the stream's failure delivered | Nothing equivalent | ce191b8c |
| Messages from others arrive between its steps, also mid-task | Her rooms' messages arrive; a peer's DIRECT message (`airc msg --to <her id>`) never reached her prompt (Aiko, 2026-11-11 14:10Z: absent from her next request; she rebased anyway); mid-act delivery: verify | direct messages reach her: card |
| The human interrupts mid-task, and is answered | Typed asks through positron; as an interrupt: verify | verify |
| A failure carries its evidence (raw output, path, line, what to try) | Typed refusals with next moves for some; bare strings for many (`stringly_command_errors` ratchet) | d1ab6ea3 |
| Working state survives a context restart (a summary carries the plan and the files in hand) | A view turnover drops her plan and what she read; she re-reads (Aiko, Aris: 0 edits in 14 acts) | 89655d88 |
| Lessons written by the agent itself persist across sessions | Genome and engrams; a reviewed correction kept across restart is not yet proven | learning line |
| It reads its own transcript and any log or process on the machine | Access is open (her shell is the operator's); nothing points her there | d1ab6ea3 |
| A thought ends in an answer; effort is set per task | A thought can hit the output cap and the act is lost (Aiko 09:03Z, 7,170 tokens) | f4a89406 |
| Time: it knows how long it has been and is told when it has gone quiet | verify | verify |
| It starts helpers in parallel and their results arrive as notifications | verify | verify |

**Status has two columns, kept apart** (Codex, 2026-11-11): *implemented* (the code exists) and
*proved live* (measured on a live node in an ordinary activity). A row closes only when proved live;
an unknown stays open until measured.

- A completion push existing is not parity: it must reliably reach her, name the activity that owns
  the work, and survive a restart, or report the loss with its cause.
- A correction becoming a gene is "better" only when an independent evaluation shows the improvement,
  her unrelated abilities stay intact, and the adoption survives a restart. Unproven today.

**The live acceptance test** combines the channels in one ordinary activity, not one test per row: she
starts background work, switches to another activity, receives a failure or an interruption, inspects
its evidence, revises her plan and finishes, with her plan and work continuous across a turnover and
a core restart. Done means she noticed, investigated and recovered by herself, on the same feedback
Claude relies on.

Better than Claude, where we can: the outcome of every channel is typed (a refusal names its cause
and next moves), the record of her own mind is one call away for her and for any other mind, and a
lesson from a correction becomes a gene, not a note.

Gate for the whole table: each row is "none" or "better", each measured on a live node in a
non-continuum activity (another project, any language, or no code at all).

## The three general problems under the gaps (2026-11-11)

Joel, 2026-11-11: "See the fundamental general problem and nuances as adaptations." Every gap found
that day was one of three problems; each card is an adapter of one mechanism, never its own fix.

1. **One fact, one owner, read live.** A stored copy drifted from the truth it copied: her window
   (a stored "available" over her live grant, #5938), where she works (shell directory vs file-verb
   root, card 566e390e), what memory is need (a cache counted as serving's need, 494c99f1), a grant
   (a standing share vs her act's need, 7ce65258), the CI base (the last push vs canary, #5941).
   Mechanism: each fact has one owner and every reader reads it live; any copy is a cache that
   knows it can be stale; an adapter holds only what differs.
2. **Everything addressed to her arrives, with a handle.** A peer's direct message never reached her
   (2ec7abf1), a cut notice pointed nowhere (#5942), failures carried no evidence (d1ab6ea3), a
   command's end was polled for (#5939, ce191b8c). Mechanism: one delivery path for messages,
   process ends, refusals and cuts, routed on the typed recipient header, waking her as an interrupt
   in the activity it concerns, carrying a handle to its evidence. Each event kind is an adapter that
   says only how to name its evidence.
3. **Nothing holds what it is not using.** A cache held as need, a learner holding memory through an
   act (#5932), a fence holding the engine lock across a wait (7fc7e2ce), standing grants, a slot's
   KV between acts. Mechanism: a resource is held for the use that needs it and given back when the
   use ends; caches are always reclaimable, justified by predicted reuse; one reclaim path for every
   holder. Each resource kind (KV, prompt cache, learner shadow, lock) is an adapter for how its
   bytes go back and come back.
4. **A result says what it verified, and nothing more.** Glass-boxed on the M5 (2026-11-11,
   16:20-16:52Z), three resident submissions failed review for one reason: every tool checks FORM and
   nothing checks MEANING. Aiko's edit removed an `if` header and the file still parsed, so the edit
   gate (syntax only) then called her correct repair "would have made it unparseable"; Benchy
   submitted a test that panics on its first `unwrap` without running it; Aris submitted three
   methods with no caller. A frontier agent checks meaning by habit (it runs the project's checker
   and reads the result before calling work done). Mechanism below; each tool, language and project
   kind is an adapter, never a case.

## One outcome for every tool (the seam for problem 4)

Today a tool's result is `ToolOutcome { success: bool, content: String, error: String }`
(`continuum-cognition/src/cognition/tool_executor/types.rs`): each tool writes its own prose about what
it did and checked, so "parses" can read as "fine", and a check nobody ran is silent. Refusals already
have the right shape (`continuum-kernel/src/sdk_codegen/refusal.rs`: a typed cause and `NextMoves`).
Every outcome takes it:

```
Outcome {
    effect:     what changed, each change by handle (file revision, process, card, message),
    evidence:   [Evidence { kind, verdict: Passed | Failed(fault) | Running(handle), output: handle }],
    unverified: [kind],          // what this effect invalidated and nothing has checked yet
    next:       NextMoves,       // the same type a refusal carries
}
kind = Form | Build | Test | Lint | Run | Review   // open: a new kind is a new variant, never prose
```

**Built on the owners that exist, never beside them.** A command already declares its effect class
once (`continuum_base::tool_effect::ToolEffect`: Observes, Changes, Reopens, Undeclared; read by the
view's repeat rule), and a refusal already carries `NextMoves`. The outcome extends those: a `Changes`
command also declares which evidence kinds its effect invalidates, beside `EFFECT` on its spec; the
evidence kinds live on the floor next to `ToolEffect` so the kernel and the view both read them; the
per-call `effect` above is the instance of that declared class.

**Rendered once.** One renderer turns an `Outcome` into her text (her text is rendered from typed
data, never authored prose). "Build: unverified" is printed because the field is there, so no tool can
word a syntax pass as "the file is fine".

**Checks are declared adapters, never tool code.** A `Checker` adapter knows one kind of project and
what it can verify: `Form` per language (the parsers `validator_for` already holds), `Build`/`Test`/
`Lint` per manifest (Cargo.toml, package.json, pyproject.toml, go.mod, a docs linter, a recipe's own
declared command). The workspace's checkers are DATA: detected from its manifests or declared by its
recipe; a workspace with none says so (`unverified`, never a guess). A tool declares only its effect;
the effect names what it invalidates (an edit invalidates `Form` for the file and `Build`/`Test` for
the project that owns it). A new tool, language or project kind is one adapter or nothing.

**Checks run as tasks, results arrive as events.** The checker scheduler runs what the effect
invalidated as background task handles (never awaited in the act, never polled); each terminal result
is an event addressed to her (problem 2), updating the `evidence` of the outcome it belongs to. On
submit, the outstanding checks of the submitted head are run and their results are part of the
submission, so a failure reaches her before a reviewer does.

**Delivery is problem 2's path, never per feature.** Check results, review verdicts, process ends and
refusals reach her view by her typed address. A review verdict filed on a card is delivered to the
author because it is addressed to her (card 759556ba), not because someone found her room: on
2026-11-11 three verdicts sent on airc never reached the residents (Benchy's ledger, 11 minutes later:
"unknown: whether Fable passes").

**The contract's invariants** (Codex's review of this design, 2026-11-11 17:00Z; each is pinned by the
outliers' tests before any migration):
1. **Evidence binds what it checked**: the exact file or tree revision, the checker's command, config,
   environment and scope. A result for an older revision never marks a newer edit verified. Verdicts
   are `Passed | Failed(fault) | Running(handle) | Inconclusive(cause)`: a cancelled, lost or
   infrastructure-failed check (a runner's DNS, a dead checker process) is Inconclusive, never Failed
   and never a quiet Running.
2. **The effect reports partial and unknown changes too**: a tool that fails partway says what it
   changed and what it cannot tell, not only what succeeded.
3. **Raw output is never deleted**: stdout, stderr, exit status and artifacts stay reachable by handle;
   the renderer wraps evidence, and migrating a tool never drops what it used to show.
4. **A pass proves its check, nothing more**: Build/Test passed means that declared check passed, not
   that the change is correct, good, or free of dead code. The renderer says which check.
5. **No new scheduler**: checks run under the existing task and resource authority (the same handles
   her shell work uses); a check superseded by a newer edit is cancelled with its process custody
   preserved (owner-lost vs confirmed-dead, card ce191b8c).
6. **A submission's evidence is Pending until the exact head's checks are terminal**: published at
   once as Draft/Pending, never as verified; "a failure reaches her before a reviewer" holds only when
   the review waits on terminal evidence. A workspace with no checker, or docs only, is Unverified,
   never Failed. A manifest alone does not declare a command (pyproject.toml does not mean pytest):
   a checker is declared by the recipe or the workspace, or detected only where the manifest itself
   names it.

**Build order** (outliers first, per the methodical process):
1. Outlier A: `code/edit` on a Rust project, `Form` (syn) + `Build` (cargo) evidence.
2. Outlier B, maximally different: a Python project with no build step (`Form` + `Test` via pytest)
   and a docs-only workspace (`Lint` or nothing). If one `Outcome` fits all three without forcing, the
   interface is proven; otherwise redesign before step 3.
3. The derive: `ToolContract` gains the effect declaration; existing tools migrate; their hand-written
   result prose is deleted, and a ratchet counts tools still returning a bare string.
4. Submission evidence, then verdict delivery through the same path.

**What it deletes:** per-tool success/failure prose, the edit gate's own wording, `ToolOutcome`'s
`success: bool` + strings, and hand delivery of reviews. Card 3fafcc2c (owner Fable).
