# Inactive engine publication repair

Card: `1a092ed2-32d3-4127-b451-8aaf161c81a2`.

The public Windows `install.ps1 -PrepareOnly` at `1bfd129400ad` verified
the published archive and prepared its core, then refused engine-c because
overlay copying retained five old DLLs outside the publisher's 15-file receipt:
`curand64_10.dll`, `nvblas64_12.dll`, `nvrtc-builtins64_129.dll`,
`nvrtc64_120_0.alt.dll`, and `nvrtc64_120_0.dll`.

`windows-engine-receipt.ps1` now owns replacement of the complete application
namespace. Both hashing and replacement use `Get-CoreEngineCandidates`; the
public prebuilt adapter delegates publication instead of maintaining its own
copy loop. The adapter rechecks the exact selected idle slot using the existing
selector while the public installer holds its existing install lease. Source
receipt validation and destination ordinary-leaf/write preflight precede changes.
Publication writes the existing pending marker, copies declared files, retires
only obsolete application leaves, and commits only after exact membership and
hash validation. Failure retains pending state; normal retry repairs it.
Unrelated metadata remains intact. No recursive slot cleanup is introduced.

The fixed engine-a/b/c contract is retained. Arbitrary immutable candidate paths
would require changing the shared prepared-release and core slot contracts;
guarded replacement reuses their existing pending-publication mechanism.

Validation: the existing Windows PowerShell 5.1 service fixture passes all 36
groups. Its engine receipt scenario now seeds those five obsolete names, refuses
a held input before mutation, injects a partial copy failure, verifies pending
receipt refusal, retries to exact membership, preserves a nonapplication file,
and exercises public adapter refusal when idle-slot selection changes. Existing
receipt/hash and mapped-image scenarios remain. `git diff --check` passes.

Evidence: local `state/team-proof-20260921/20261009-inactive-engine-tests.log`.
This is source/fixture evidence only. No live engine files, jobs, selected slots,
services or receipts were manually changed. Matching prebuilt publication and
another supported public preparation remain required before adoption.
