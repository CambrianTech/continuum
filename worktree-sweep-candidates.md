# Worktree-forest sweep — Phase 1 candidate list (2026-10-29)

Card: bf7aa2f6 "Worktree-forest sweep: prune checkouts whose card is terminal (candidate list first, operator confirms)"

## Method & cross-checks

- Census of `C:\Users\joelt\.airc/worktrees` at 13:05Z: **936 entries** = 412 dirs + 524 stray files.
  - Stray files total only ~82 MB (`.log/.md/.patch/.txt` scratch dropped into the shared root) → **report-only, never deleted by this sweep**.
- Dir split: ~305 card-named (8-hex short id), ~10 suffixed (`<id>-…`), ~102 named working dirs (`astra-*`, `codex-*`, `baseline-canary`, …) + dot-dirs (`.allocating/.locks/.migrating/.retired`).
  - Named working dirs + dot-dirs are **structurally excluded** — not derivable from card state.
- Authoritative dry-run: `airc work cleanup --dry-run` (pid 3012) → scanned all 315 registered worktrees; coverage cross-checked against on-disk census: **every card-named dir appears in the report (notScanned=0)**.
- Independent board-state map built from board dumps (`continuum`, `general`) and joined with the tool's classification. Agreement is exact where both sides can see: **TERMINAL(board) ∩ Removable(tool) = 14, zero disagreements**.

## Category A — clean deletion candidates (23 dirs; tool says "would remove on --force")

Terminal card state confirmed by two independent sources except where noted.
`[B]` = board not visible to me from this scope; terminal state per substrate resolution only.

| dir | PR | card state | size |
|---|---|---|---|
| 08551ce5 [B] | — | Closed | pending |
| 1acc9097 | #5438 | Closed | pending |
| 1ad80827 [B] | — | Closed | pending |
| 1fc73f18 | #5398 | Closed | pending |
| 2aa980e5 | — | Closed | pending |
| 3f37271f [B] | #3 | Merged | pending |
| 48d11f33 | — | Closed | pending |
| 4a860043 | — | Closed | pending |
| 57d51f97 | #5580 | Merged | pending |
| 5e7dec94 [B] | — | Closed | pending |
| 6892d084 | — | Closed | pending |
| 6afd2598 [B] | — | Closed | pending |
| 786cc36d | — | Closed | pending |
| 8410d45d | — | Closed | pending |
| 97851a9a | — | Closed | pending |
| 9827e35d | — | Closed | pending |
| a91eee61 | — | Closed | pending |
| a93eeafe [B] | — | Closed | pending |
| cb4dea0f | — | Closed | pending |
| cff534ba | — | Closed | pending |
| f38fe65e [B] | — | Closed | pending |
| f46fd2f9 [B] | — | Closed | pending |
| f4b92f4a | — | Closed | pending |

## Category B — terminal card, but checkout holds uncommitted/unpushed work (22 dirs) → operator judgment: salvage or discard

All `KeepActive` per tool with reason "uncommitted or unpushed work" (20) or "not a git worktree" (2).

| dir | PR | card state | note |
|---|---|---|---|
| 0c69aa07 | — | Closed | uncommitted/unpushed work |
| 0e0bbf21 | — | Merged | uncommitted/unpushed work |
| 0e8aaa43 | #3876 | Merged | uncommitted/unpushed work |
| 172f352f | — | Closed | uncommitted/unpushed work |
| 2e2a92f6 | #3922 | Merged | uncommitted/unpushed work |
| 2f004ea2 | — | Merged | uncommitted/unpushed work |
| 500d241b | — | Closed | uncommitted/unpushed work |
| 519dafdb | #3886 | Merged | not a git worktree |
| 7028c695 | #5052 | Closed | uncommitted/unpushed work |
| 7c72c0f0 | — | Merged | uncommitted/unpushed work |
| 8bd75362 | — | Merged | uncommitted/unpushed work |
| 8e4c42de | — | Closed | uncommitted/unpushed work |
| a5d4c876 | — | Closed | uncommitted/unpushed work |
| aa5888a9 | #3922 | Merged | uncommitted/unpushed work |
| b742642d | #3869 | Merged | not a git worktree |
| c34034b9 | — | Merged | uncommitted/unpushed work |
| c5910be2 | #3915 | Merged | uncommitted/unpushed work |
| cf865878 | — | Closed | uncommitted/unpushed work |
| d2698cdc | #3896 | Merged | uncommitted/unpushed work |
| d3a05224 | #5549 | Closed | uncommitted/unpushed work |
| d62743d6 | — | Closed | uncommitted/unpushed work |
| e6a1b2cb | — | Merged | uncommitted/unpushed work |

## Category C — orphans (19 dirs): no matching card on any board → report-only, never Phase-1 delete

`265f7207 287c1bd9 2c393644 450b006f 59b79686 6c0e3b1f 7d890bb3 7de89e05 937ace56 9f91198f b1e80b38 b7053790 bca5bcb3 cbcf9529 cde631d4 d0c9450a e8cb6e7e f3142525 ff7da522`

## Also noted (not in scope)

- `SkipDetached`: `cbaf84de` — PR #5137 Merged but detached HEAD; tool retains checkout.
- ~272 KeepActive dirs with active card state — untouched.
- 524 stray files (~82 MB) at forest root — report-only.

## Ask of operator

Confirm Category A for deletion (or mark exceptions). Category B needs a salvage/discard decision per dir if it is to be touched at all; otherwise it stays as-is. Category C: confirm whether orphans may ever be deleted, and under what rule.
