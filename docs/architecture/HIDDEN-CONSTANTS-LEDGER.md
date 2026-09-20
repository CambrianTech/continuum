# Hidden Constants Ledger — every number that silently shapes cognition

**Born 2026-08-23.** `DEFAULT_MAX_ACTS = 8` sat unnamed in eval.rs and graded a
citizen 0-pass with an empty `src/` after 8 competent analysis acts — the cap
measured our patience, not the model, and nothing in her perception ever said
the floor existed. Joel: *"more of your damn hidden constants."* This ledger is
the standing sweep of that class.

## The law

A numeric constant that shapes what a citizen can perceive, recall, hold, or do
must be ONE of:

1. **Window-derived** — expressed as a fraction of the served context
   (`ContextBudget` denominators). Scales with the hardware; no LCD clamp.
2. **Recipe/data-owned** — carried by the task row, activity recipe, or persona
   config (`EvalTask::max_acts` is the model). The gym sizes patience to the
   task class; the code carries only the inherit default.
3. **A justified bound** — a backstop against runaways (never the working
   ceiling), documented at the site with the incident or invariant it guards.
   The test: honest work must never hit it.

A bare capability cap — a number that decides how much mind a citizen gets,
living in code, invisible to her, unscaled by hardware — is the forbidden
shape. It is exactly how 8 acts, a 2,816-token window, and the two-tool
discovery pair each muted capable models.

## Classification (swept 2026-08-23, cognition/ + persona/)

### Fixed this sweep

| Constant | Was | Now |
|---|---|---|
| `eval::DEFAULT_MAX_ACTS` | 8, silent | 32, per-task `max_acts` override, and the act-budget proprioception facts make the stopwatch VISIBLE to her |

### Capability caps needing principled fixes (worst first)

| Constant | Value | Why it's suspect |
|---|---|---|
| `persona_workspace::DEFAULT_WORKSPACE_CAPACITY` | 6 | SIX global-workspace slots for every citizen on every hardware tier. Should scale with window (a 160k lane can hold more mind than a 4k lane) or be persona-config. |
| `recall_faculty::DEFAULT_RECALL_LIMIT` | 16 | Recall breadth capped identically for a 3B and a 35B. Window-derived candidate (`RECALL_DENOM` exists in context_budget — route through it). |
| `channel_digest::DEFAULT_GROUNDING` / `FIRST_READ_PAGE` | 5 / 20 | How much of a room she perceives on entry. Data-owned candidate (room recipe). |
| `response_orchestrator::DEFAULT_RELEVANCE_THRESHOLD` | 0.30 | Speaks/silence gate. Should be persona-learned or config, not universal. |
| `check_redundancy::REDUNDANCY_CONVERSATION_WINDOW` | 10 | Redundancy horizon fixed at 10 turns regardless of window. |
| `recall_faculty::RECALL_RELEVANCE_FLOOR` | 0.15 | Universal relevance floor; interacts with embedding model choice. |

### Justified bounds (audited, keep — each documents its incident)

- `settle::STUCK_LIMIT = 3` — byte-identical act loop breaker (#206); genuine
  iteration has a different signature and never trips it.
- `eval::PER_TASK_HANG_BACKSTOP = 2h` — firing is a bug report, not a policy.
- `service_loop::LIVE_MAX_ACTS = usize::MAX` — the CORRECT shape: no cap, she
  settles when done.
- `discovery_budget = max_acts / 2` (#390) — derived from the act budget, lifts
  on first mutation, only bounds pre-write wandering.
- Spill/truncation caps (`tool_executor`) — context safety with full recovery
  on disk (`tool/output`); nothing is lost.
- `context_budget` denominators — the sanctioned window-derived shape itself.

### Infrastructure tuning (not cognition-shaping; ordinary review)

Cache sizes (`EMBEDDING_CACHE_MAX`, `CHANNEL_ELEMENT_CACHE_MAX`), schema
versions, fetch paging (`MAX_ROWS`), display truncation for ledgers.

## Enforcement

The de-hardcode guard (`no_new_hardcoded_context_or_prompt_size_constant…`)
already refuses new `WINDOW|CTX|TOKEN|PROMPT|CHARS`-named literals. This ledger
is the review-time companion for the names that guard cannot match: when a PR
adds a `const` that decides how much a citizen perceives, recalls, holds, or
acts — demand shape 1, 2, or 3, in that order of preference. When one of the
"needing principled fixes" rows lands its fix, move it to the fixed table with
the PR number.
