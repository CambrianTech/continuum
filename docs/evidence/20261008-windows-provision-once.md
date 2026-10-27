# Windows supervisor provision-once migration — card 5dc5cdc1

## Problem and owner

A normal caller had the installer-provisioned read/write/execute task grant, yet updating the installed S4U boot task failed with 0x80070005. Each release changed the task action and Description. The PowerShell registrar granted RWX while the Rust registrar granted RX, so even their authority contract diverged.

The installer now provisions fixed core/deploy actions once. A protected ProgramFiles bootstrap and its complete declared DLL closure belong to administrators/System; no mutable release is selected before a positively verified Medium-or-lower token. Routine installs publish a bounded, hash-validated active release receipt under the installation lease. Tasks retain their principal, triggers, enabled intent and action. Pending preparation remains distinct from active selection.

The existing prepared-release owner also owns active/previous receipt validation, atomic replacement, legacy initialization and compare-and-restore. Native and browser staging use the shared inactive-slot owner; the superseded native per-file staging and task-action update paths are removed. Runtime DLL declarations travel with their binaries, and reusing an inactive slot invalidates a pending receipt before changing its bytes.

## Migration and failure behavior

A first legacy migration seeds the currently selected release before changing task authority, then selects the candidate only after both tasks and protected bootstrap verify. This preserves a prior selection, not a claim that that release was healthy. Partial task registration restores both prior task definitions. A failed stopped candidate may restore a validated previous selection only if the active descriptor is unchanged; an answering core or running supervisor is preserved. Missing/corrupt schema 2 receipts never select a legacy fallback.

## Evidence and limits

The existing PowerShell 5.1 fixture passes, including prepared/active separation, two receipt selections, runtime DLL tampering, first legacy selection, compare-and-restore, stale pending invalidation, partial task-registration rollback, SID/path refusal and caller delete-child authority refusal. This is contract/adapter fixture evidence, not two actual installed Windows handoffs.

Independent source/security review and Rust validation are tracked on the card. Actual acceptance still requires a published compatible prebuilt, the ordinary public installer from the old registered state, then two medium-token handoffs plus recovery without re-registration/elevation. No private rollback script, hand-seeded receipt, unpublished worktree helper, or fixture result substitutes for that installed evidence. No live service changes were made to validate this patch.

Validation receipts before final canary integration: all 69 lifecycle tests passed in 10.90 s (including the actual Windows Safer Medium-token fixture); all 28 source-hygiene tests passed in 3.75 s; core CLI cargo check passed in 38.15 s. The PowerShell 5.1 fixture passed 32 scenario groups. Current canary through c998b369b (including embedded AIRC and model-load validation) was merged as 21dfafe09; final integration verification is recorded with the pull request. These are build/test receipts, not installed node recovery or learning evidence.

Integrated canary checks: 71 lifecycle tests passed in 10.79 s, strict lifecycle all-targets Clippy passed in 2.16 s, and core CLI cargo check passed in 2 min 22 s. The final supervisor dispatch retains the existing hashed media launcher after token lowering; its final core CLI cargo check passed in 35.03 s. Independent source review approved exact code commit 9c29374264f0e2cbc9b088520bf6e54162c670f9. The final PowerShell 5.1 fixture passed all 32 scenario groups after integration.
