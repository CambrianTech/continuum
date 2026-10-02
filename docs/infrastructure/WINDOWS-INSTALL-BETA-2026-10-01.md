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

## Payload contract under implementation (draft, no live acceptance)

`paths::payload_root` and the platform readers share a UTF-8 `payload-root`
record in the hot Continuum home. Without a record, existing installs retain
their home layout. A recorded missing/unreadable/invalid target fails closed;
it must never become another installation on the system drive. Fresh selection
uses a home-scoped directory under the already selected cold root. Existing
`bin`, `tools`, `lib` or `cuda-toolkit` trees retain their original location.
Reruns read the record rather than selecting storage again. Engine slot state
stays together under the selected payload `bin`; identities, room/session data,
logs and configuration remain in the hot home.

The first source slice adds the runtime resolver, engine launch/error handling,
`engine root` query, and platform selection/read primitives with regressions.
Both public installers now invoke selection after cold-cache configuration and
before prerequisites. Windows tool/library destinations, service/engine slot
adapters, prepared-release validation, Unix engine installation, manifest runtime
paths and direct/scripted library environments consume the record. Service-host
bootstrap validates against the selected root and propagates registration failure
instead of changing managed engines into an operator pin.

The ordinary PowerShell entry runs in a scratch profile and proves selection at
the prerequisite boundary without acquisition. The full PS5 suite also covers
cold prepared-release validation and engine preparation. Bigmama independently
proved native-path cross-reading both ways between PS5 and Git Bash with spaces
at the initial contract revision, including extended drive paths and unavailable
target refusal. UNC share coverage remains unavailable. Bigmama's consumer and
lifecycle review at `0340a6e73` found no blocker in the inspected integration;
approval remains subject to required CI and actual installer acceptance.
Linux CI exposed a warm-build scratch fixture missing the launcher's new shared
helper. The fixture now copies the actual helper, and its Windows/Linux/macOS
control-flow regressions pass in Git Bash. Final runtime compilation/tests and
public rerun remain required. Linux CI also includes cold engine-slot and
manifest-loader regressions.
Public install, reboot/rerun, remote command/event and GPU proof remain OPEN.

The follow-up consumer audit found additional fixed hot-home paths in Unix core
publishing/service resolution, CLI core lookup, macOS install staging and engine
orphan ownership. These now consume the selected payload root too; macOS receives
the resolved slot from the CLI rather than duplicating path policy in its adapter.
The existing artifact-order and plist tests cover a cold slot, and shell placement
tests verify that an unavailable selected directory refuses service operations.
This additional consumer change requires fresh CI and peer review.

## First public Continuum run (canary 84c18e611)

After all required CI and independent review passed, Joel confirmed readiness.
The unmodified PowerShell 5.1 `install.ps1 -Grid` selected D: cold cache/payloads,
restored registered Rust tools, reused Visual Studio, cloned native submodules
and installed CMake. No admin operation or consent prompt was reached.
LLVM's 981,666,720-byte archive passed the manifest checksum, then Windows tar
failed because its external XZ decoder was absent. The installer exited nonzero;
no manual extraction or installation was performed.

The repair declares the official Windows XZ archive and digest in the manifest,
acquires/verifies it under managed payload storage, and supplies it only to the
native tar invocation. Extraction checks its exit status and stages LLVM before
publishing files. A Windows smoke test uses the actual decoder and native tar in
an isolated profile, verifying acquisition/reuse, corrupt-archive refusal and
PATH restoration. Public rerun and full Continuum acceptance remain required.
The lengthy LLVM download also exposed missing useful progress in captured
installer output; download observability remains an open usability repair.

The public rerun on canary `355c84833` acquired XZ automatically, then stalled
inside Windows tar's external decoder filter. Two process samples showed tar
fixed at 134,080 bytes read / 65,536 written and XZ at 73,728 read / zero written,
with unchanged CPU counters and no extracted files. The follow-up avoids that
filter: XZ reads the archive directly, its binary stdout is copied to a temporary
tar file while stderr drains independently, then native tar reads the plain file.
The regression now uses 2 MB of incompressible data rather than a tiny string.
That larger smoke passes in an isolated profile using the checksum-verified
decoder archive. Full live-archive acceptance remains open.

During this run the agent environment changed to restricted permissions. CIM
process inspection returned Access denied, so the ownership-checked stop refused
to act; the old installer was not stopped. GitHub CLI subsequently reported its
keyring credential unusable. No authentication reset, privilege workaround or
second installer was attempted. Restore task access and inspect the owned process
before any rerun; do not infer the old run has ended.
