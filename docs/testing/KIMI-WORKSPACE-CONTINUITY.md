# Kimi workspace continuity receipt — 2026-10-04

Kimi's Slice2 card is `74ec9613-7bbf-4f80-819a-ccc5c844a932` in Career Wrangler.
She committed independent review corrections as
`5184f8568d38492ce8f0bf88a0244f7c52f57b53`. At 2026-10-03 18:47:02-0500,
arrival reset her branch to remote `4daeb84699fe197c36b768e5f5ae4d343761b8ac`.
The latter is an ancestor of her correction commit, not a divergent successor.
Installed core `d8bca290e` contains this arrival policy. The recovery ref is
`refs/continuum/stranded/74ec9613/slice-2-server-outbox-dispatcher-node-1791071221716`.

The existing responsibility owner, `persona/workspace_transfer.rs`, now compares
both immutable tips. On the exact requested branch, a fetched tip that adds no
commits preserves local successors and dirty work. The `LocalWorkPreserved`
receipt identifies both tips, local commit count and dirty state. Genuine divergence
continues to save a recovery ref and transfer to the remote tip. `card_staging`
continues to use this shared owner after the authenticated claim.

Before: repeated arrival can reset an unpublished successor and strand its fixes.
After: repeated arrival retains HEAD, staged index, unstaged files and untracked
files. The existing two-machine fixture verifies those facts and retained genuine
divergence, unreachable-remote and cross-machine transfer behavior.

Validation: session86485 in the integration checkout passed33 tests with2 ignored,
including all6 workspace-transfer tests. The workspace-transfer source copied to
this canary candidate is identical after newline normalization. Independent local
diff review found no blocking issue. Exact canary CI and deployed acceptance remain
owed. Kimi's application worktree was observed only; her recovery and new submission
remain hers, and Fable retains independent application review authorship.

Branch publication follows the same owner and immutable-tip ancestry contract.
Before: HEAD reachable from any origin branch was treated as published, although
the receiving node fetches the exact card branch. After: both sync and placement
require publication to that branch; a missing card branch needs a push even if its
base already exists on main. Deleted the all-origin reachability counter and
shared history_counts with arrival. The existing two-machine setup verifies a
missing card branch, a successor on another remote branch, blocked placement,
successful card push and the receiving node's actual file content.

The generic publication defect is source-confirmed; it is not established as the
cause of Kimi's missing push. Read-only observation at 01:32 UTC found her checkout
restored to clean5184f8568; reflog records reset to that tip at01:11:37 UTC. No parent
application edit or recovery was performed. Recovery actor, fresh tests and revised
submission remain unverified. Independent review and final scoped validation follow.

Follow-up validation: parent session80768 passed all7 workspace-transfer tests on
the final source in the integration checkout; candidate source was copied exactly.
Independent scoped review found no introduced correctness blockers. Fable's earlier
approval covers4cfd13052 only; renewed exact-head review and canary CI remain owed.
No install or deployed continuity claim.
