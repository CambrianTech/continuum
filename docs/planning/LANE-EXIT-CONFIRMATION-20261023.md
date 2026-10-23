# Confirm lane exit before completing maintenance

Card cde631d4-45e5-4a65-97e6-005c0a1fbe30; successor to #5307.

Published 6b1f511bb passed Scheduler custody on this Windows machine. The supported maintenance command durably stopped 65 modules (706 ms), requested termination of owned engine PID4280, then refused an immediate `ExitUnconfirmed` snapshot. The 60-second observation never began. Later startup reported AlreadyGone4280; core5a765f7a1 and engine600 returned ready with the same two Kimi adapters. ContinuumDeploy was resumed and remains disk-admission blocked. This is not capacity recovery or successful maintenance.

Raw local receipts: `C:/Users/joelt/.continuum/state/team-proof-20260921/d0c9450a-installed-maintenance.log` and `d0c9450a-restored-serving.json`. No repeated maintenance, deployment, local compilation, or runtime-state edits accompany this change.

Shutdown now stages the existing sweep off the async worker and reuses `lane_process::confirm_gone` with its existing KILL_POLL bound. Only incarnations verified before signalling qualify for delayed confirmation. Unknown/legacy identities and true survivors remain unconfirmed. The registry lock is released while waiting; retirement compares the exact original record under the same write lock, preserving concurrent replacements. Boot sweep behavior stays unchanged. No new scheduler, repeated signal, or assumed exit is introduced.

The existing registry test module is extracted intact below the file-size boundary and adds a real re-executed child that remains alive until stdin EOF: survivor retention, delayed exit, recycled incarnation, and replacement-record preservation through the same pending-outcome reconciliation used by shutdown. The Windows job explicitly executes this fixture and requires one passing test; the normal Unix engine suite includes it. Validation is pending CI, not claimed from source inspection. The original failure is consistent with asynchronous termination but the later absence alone does not prove termination authority at the earlier instant.
