# Worktree-forest sweep — Phase 1 candidate list (2026-10-29)

Card: bf7aa2f6 "Worktree-forest sweep: prune checkouts whose card is terminal (candidate list first, operator confirms)"

## Method & cross-checks

- Census of `C:\Users\joelt\.airc/worktrees` at 13:05Z: **936 entries** = 412 dirs + 524 stray files.
  - Stray files total only ~82 MB (`.log/.md/.patch/.txt` scratch dropped into the shared root) → **report-only, never deleted by this sweep**.
- Dir split: ~305 card-named (8-hex short id), ~10 suffixed (`<id>-…`), ~102 named working dirs (`astra-*`, `codex-*`, `baseline-canary`, …) + dot-dirs (`.allocating/.locks/.migrating/.retired`).
  - Named working dirs + dot-dirs are **structurally excluded** — not derivable from card state.
- Authoritative dry-run: `airc work cleanup --dry-run` → scanned all 315 registered worktrees; coverage cross-checked against on-disk census: **every card-named dir appears in the report (notScanned=0)**.
- Independent board-state map built from board dumps (`continuum`, `general`) and joined with the tool's classification. Agreement is exact where both sides can see: **TERMINAL(board) ∩ Removable(tool) = 14, zero disagreements**.
- Re-verified 2026-10-30 by an independent re-run (`airc work cleanup --dry-run`, AIRC_HOME pinned to the machine home; identity.key SHA-256 `cd320fd9…` unchanged before and after): same **315** registered worktrees scanned — Removable set **identical 23/23** vs Category A, orphan (SkipUnknownCard) set identical 19/19 vs Category C, SkipDetached still `cbaf84de`.

## Category A — clean deletion candidates (23 dirs; tool says "would remove on --force")

Terminal card state confirmed by two independent sources except where noted.
`[B]` = board not visible to me from this scope; terminal state per substrate resolution only.
Sizes measured 2026-10-29 (`du -s -B1`). **Total: 38,687,195,136 bytes ≈ 36 GiB.**

| dir | PR | card state | size |
|---|---|---|---|
| 08551ce5 [B] | — | Closed | 2,066,562,048 (~1.9 GiB) |
| 1acc9097 | #5438 | Closed | 2,064,607,232 (~1.9 GiB) |
| 1ad80827 [B] | — | Closed | 2,066,586,624 (~1.9 GiB) |
| 1fc73f18 | #5398 | Closed | 2,063,787,008 (~1.9 GiB) |
| 2aa980e5 | — | Closed | 2,064,992,256 (~1.9 GiB) |
| 3f37271f [B] | #3 | Merged | 1,786,880 (~1.7 MB) |
| 48d11f33 | — | Closed | 2,068,180,992 (~1.9 GiB) |
| 4a860043 | — | Closed | 2,065,099,776 (~1.9 GiB) |
| 57d51f97 | #5580 | Merged | 2,069,456,896 (~1.9 GiB) |
| 5e7dec94 [B] | — | Closed | 2,066,574,336 (~1.9 GiB) |
| 6892d084 | — | Closed | 2,065,049,600 (~1.9 GiB) |
| 6afd2598 [B] | — | Closed | 2,066,744,320 (~1.9 GiB) |
| 786cc36d | — | Closed | 2,067,789,824 (~1.9 GiB) |
| 8410d45d | — | Closed | 2,104,652,800 (~2.0 GiB) |
| 97851a9a | — | Closed | 2,066,354,176 (~1.9 GiB) |
| 9827e35d | — | Closed | 243,009,536 (~232 MiB) |
| a91eee61 | — | Closed | 243,009,536 (~232 MiB) |
| a93eeafe [B] | — | Closed | 2,066,590,720 (~1.9 GiB) |
| cb4dea0f | — | Closed | 2,066,354,176 (~1.9 GiB) |
| cff534ba | — | Closed | 2,068,328,448 (~1.9 GiB) |
| f38fe65e [B] | — | Closed | 482,080,768 (~460 MiB) |
| f46fd2f9 [B] | — | Closed | 482,080,768 (~460 MiB) |
| f4b92f4a | — | Closed | 2,067,516,416 (~1.9 GiB) |

Note: the ~1.9 GiB uniformity is expected — each dir is a shallow clone of `CambrianTech/continuum`; `.git` objects dominate and working trees are small.

## Category B — terminal card, but checkout holds uncommitted/unpushed work (22 dirs) → operator judgment: salvage or discard

All `KeepActive` per tool with reason "uncommitted or unpushed work" (20) or "not a git worktree" (2). Per-dir sizes measured 2026-10-29 (`du -s -B1`). **Total: 18,456,631,296 bytes ≈ 17.2 GiB.** None are Phase-1 delete candidates regardless.

| dir | PR | card state | size | note |
|---|---|---|---|---|
| 0c69aa07 | — | Closed | 514,776,064 (~491 MiB) | uncommitted/unpushed work |
| 0e0bbf21 | — | Merged | 514,589,696 (~491 MiB) | uncommitted/unpushed work |
| 0e8aaa43 | #3876 | Merged | 462,394,368 (~441 MiB) | uncommitted/unpushed work |
| 172f352f | — | Closed | 514,940,928 (~491 MiB) | uncommitted/unpushed work |
| 2e2a92f6 | #3922 | Merged | 504,346,624 (~481 MiB) | uncommitted/unpushed work |
| 2f004ea2 | — | Merged | 506,915,840 (~483 MiB) | uncommitted/unpushed work |
| 500d241b | — | Closed | 2,097,696,768 (~1.95 GiB) | uncommitted/unpushed work |
| 519dafdb | #3886 | Merged | 16,384 (tiny) | not a git worktree |
| 7028c695 | #5052 | Closed | 513,985,536 (~490 MiB) | uncommitted/unpushed work |
| 7c72c0f0 | — | Merged | 506,915,840 (~483 MiB) | uncommitted/unpushed work |
| 8bd75362 | — | Merged | 514,132,992 (~490 MiB) | uncommitted/unpushed work |
| 8e4c42de | — | Closed | 2,064,598,016 (~1.92 GiB) | uncommitted/unpushed work |
| a5d4c876 | — | Closed | 506,932,224 (~483 MiB) | uncommitted/unpushed work |
| aa5888a9 | #3922 | Merged | 504,371,200 (~481 MiB) | uncommitted/unpushed work |
| b742642d | #3869 | Merged | 45,056 (tiny) | not a git worktree |
| c34034b9 | — | Merged | 506,915,840 (~483 MiB) | uncommitted/unpushed work |
| c5910be2 | #3915 | Merged | 503,977,984 (~481 MiB) | uncommitted/unpushed work |
| cf865878 | — | Merged | 2,099,632,128 (~1.96 GiB) | uncommitted/unpushed work |
| d2698cdc | #3896 | Merged | 503,900,160 (~481 MiB) | uncommitted/unpushed work |
| d3a05224 | #5549 | Closed | 2,512,270,336 (~2.34 GiB) | uncommitted/unpushed work |
| d62743d6 | — | Closed | 2,096,361,472 (~1.95 GiB) | uncommitted/unpushed work |
| e6a1b2cb | — | Merged | 506,915,840 (~483 MiB) | uncommitted/unpushed work |

Note: the ~480–490 MiB cluster is also expected (shallow clone with a small working tree); the five outliers (≥1.9 GiB) are full-depth checkouts or carry large build artifacts — those are where salvage would recover the most if any of them holds real work.

## Category C — orphans (19 dirs): no matching card on any board → report-only, never Phase-1 delete

Per-dir sizes measured 2026-10-29 (`du -s -B1`). **Total: 13,832,186,880 bytes ≈ 12.9 GiB.**

| dir | size |
|---|---|
| 265f7207 | 2,064,126,976 (~1.92 GiB) |
| 287c1bd9 | 515,026,944 (~491 MiB) |
| 2c393644 | 9,749,504 (~9 MiB) |
| 450b006f | 11,407,360 (~11 MiB) |
| 59b79686 | 8,154,112 (~8 MiB) |
| 6c0e3b1f | 514,965,504 (~491 MiB) |
| 7d890bb3 | 9,101,312 (~9 MiB) |
| 7de89e05 | 513,690,624 (~490 MiB) |
| 937ace56 | 2,064,558,080 (~1.92 GiB) |
| 9f91198f | 8,178,688 (~8 MiB) |
| b1e80b38 | 2,066,831,360 (~1.92 GiB) |
| b7053790 | 297,954,304 (~284 MiB) |
| bca5bcb3 | 60,354,560 (~58 MiB) |
| cbcf9529 | 9,757,696 (~9 MiB) |
| cde631d4 | 514,945,024 (~491 MiB) |
| d0c9450a | 514,899,968 (~491 MiB) |
| e8cb6e7e | 2,066,827,264 (~1.92 GiB) |
| f3142525 | 2,066,704,384 (~1.92 GiB) |
| ff7da522 | 514,953,216 (~491 MiB) |

## Grand totals (measured dirs only; named working dirs + dot-dirs excluded)

- **A: 38,687,195,136 bytes ≈ 36.0 GiB** — clean delete candidates
- **B: 18,456,631,296 bytes ≈ 17.2 GiB** — salvage-or-discard judgment needed
- **C: 13,832,186,880 bytes ≈ 12.9 GiB** — orphans, report-only
- A+B+C combined: **70,975,945,312 bytes ≈ 66.1 GiB** of the forest

## Also noted (not in scope)

- `SkipDetached`: `cbaf84de` — PR #5137 Merged but detached HEAD; tool retains checkout.
- The other **272** scanned dirs are: 98 `KeepActive` (active card, clean), 154 `SkipDirty` (uncommitted/unpushed work), 19 `SkipUnknownCard` (= Category C orphans above), 1 `SkipDetached` — all untouched by this sweep.
- 524 stray files (~82 MB) at forest root — report-only.

## Ask of operator

Confirm Category A for deletion (or mark exceptions). Category B needs a salvage/discard decision per dir if it is to be touched at all; otherwise it stays as-is. Category C: confirm whether orphans may ever be deleted, and under what rule.
