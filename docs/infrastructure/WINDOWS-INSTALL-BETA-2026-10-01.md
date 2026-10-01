# Windows public-installer beta: BIGGIEDESK

Owner: Codex, coordinated with Astra/Bigmama through AIRC. All repairs belong in
the public installers. Diagnostic experiments do not count as installation
acceptance, and successful reuse does not prove first acquisition.

## Prerequisite evidence

AIRC's normal standalone `install.ps1` completed twice on runtime revision
`077158a9a2ed`: automatic secondary-drive build storage, shared elevation,
firewall verification, hidden startup, installed-daemon adoption, verified legacy
endpoint retirement, and an idempotent rerun preserving the daemon PID. A fresh
two-way room challenge with Bigmama passed. Joel confirmed the old terminal
window closed. AIRC PR #1470 passed all CI and merged to canary at
`f47a06b57dfe8fc66c1d02ddb8a665800b3fbbfc`. Main promotion remains separate.

Continuum PR #4649 supplies the reviewed shared elevation and canonical AIRC
firewall delegation, merged after green CI at `1248f9073391b149e4a3ab32a110fd2e42fd3b59`.
Its fresh dependency contract requires AIRC #1470 on canary, now satisfied.
The cold-storage repair is stacked above #4649; it does not replace that contract.

## Intervention and acceptance checklist

| Finding | Repository action | Acceptance |
|---|---|---|
| Failed winget prerequisites only warned, suggested manual installs, and continued into dependent builds; caller-local native status could mask the result | Read actual global native status and throw on vendor failure or a failed post-install probe; retain verified success, reboot-required success, and healthy reuse | Regression added; live provisioning still required |
| Cold storage selected only after large Windows prerequisite downloads; system drive has about 1.8 GB free while secondary drive has about 4.2 TB | Select storage before prerequisite provisioning; route temporary download/extraction files to the selected disk for this installer session, on both platform adapters | Scratch regression passes; ordinary Continuum install still required |
| Both cold-storage adapters replaced the whole `config.env`, losing unrelated settings; Windows used ASCII | Update only storage keys, retain other settings/comments/UTF-8, stage writes before replacement | Existing Windows and shell regression suites cover preservation and repeatability |
| Windows could not reread its own single-quoted storage path; shell `xargs` could alter literal path text | Strip matching outer quotes without evaluation; retain literal spaces/backslashes/dollar signs; use last assignment like the runtime | Scratch regression passes; real configured-path rerun still required |
| Existing Codex process lacks persisted user Rust-home variables despite AIRC installing Rust on the secondary drive | Public entry restores absent Rust-home settings from user/machine registration, preserves explicit process settings, and refreshes PATH without dropping session tools or accumulating duplicates | Scratch regression added; ordinary public-entry proof remains OPEN; no agent environment override may hide this |
| CUDA toolkit and installed runtime payloads still target the system drive | Review existing storage policy and capacity before provisioning multi-GB payloads | OPEN; temporary-file routing alone does not prove sufficient space |
| Existing cold-directory migration can warn on partial robocopy failure and continue | Both adapters persist the selected pending root and own each source/destination pair before moving; failures stop configuration publication, reruns resume only owned destinations, and ancestor links/out-of-root paths are refused | Scratch partial-failure/resume and Windows junction regressions pass; POSIX symlink assertions require Unix CI (Git Bash copies link fixtures); public migration remains OPEN |
| Bigmama remote Continuum ping timed out; no installed local Continuum CLI/core was found | Complete normal `install.ps1 -Grid`, then verify the installed receiver and actual remote command/event path | OPEN; AIRC room messaging does not prove Continuum remote execution |

No live Continuum installer has run for this repair yet. Tests use isolated
scratch profiles/files and mocked elevation boundaries; they do not provision
the machine. Before a live run, announce any expected Windows consent prompt and
allow Joel time to approve it. Never claim a complete install from a build alone.

## Installed payload boundary (peer review)

Bigmama confirmed that managed engine slots currently belong to
`continuum_home/bin`: `inference/engine_slots.rs`, `llama_server.rs`, the slot CLI,
and `windows-service.ps1` all consume or enforce that location. Existing
`CONTINUUM_STORAGE_PATH` selects models and build cache, not engine slots.
`LLAMA_SERVER_BIN` is an operator override and cannot substitute for managed
placement. Relocating all of `CONTINUUM_HOME` would move hot metadata as well.

The remaining repair therefore needs one managed payload-root decision shared
by runtime/slot commands and platform installer adapters, preserving active,
previous, verified and idle-slot behavior. Changing only the Windows copy
destination would break startup, reboot, promotion and rollback. This is still
OPEN; no runtime relocation or live install has been performed.
