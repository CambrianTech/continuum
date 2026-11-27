# Bounded serving maintenance

Card: b7053790-c62b-45a6-a47a-5027f473e086 (fable-astra).

This stabilizes persistent citizens by giving coordinated development maintenance a
supported foreground lifecycle. It composes the existing durable shutdown and
Windows SchedulerCustody owners; it does not migrate containers, delete caches,
change pagefile settings, build source, or force a shutdown without durable state.

After coordinating current resident work and deployment ownership:

```
continuum maintenance 60
continuum maintenance recover
```

The first command requires a ready managed serving identity and an enabled core
task, a verified protocol-4 protected bootstrap, and no live deploy claim. It holds
the existing install and launch custody, snapshots original task XML/security and
enabled state in the existing durable journal, disables managed restarts, and uses
the existing targeted durable drain/commit. Observation starts only after core and
recorded owned-engine exit is proven. It lasts 1–300 seconds; Ctrl-C during the
observation ends the wait and restores. The signal handler is installed before
mutation so interruption during drain does not abandon restoration.

Every ordinary observation/drain failure attempts scheduler restoration. Partial
scheduler-disable failure is also restored under the same pinned authority. If
restoration fails, the original journal remains for explicit recovery; task identity
or security drift is refused rather than overwritten. A crash/forced process exit
also retains that journal. Recovery restores only non-container maintenance and
never repeats the observation. It preserves originally disabled task intent.

Restoration re-enables the exact original scheduler intent. The original core is
launched at most once through the existing registered-service owner, then its
model, native identity, revision, adapters, context geometry and vision are checked.
Scheduler restoration releases custody before startup, so a competing deployment
can make acceptance fail explicitly; it is not silently rolled back. An interrupted
process loses the in-memory serving expectation: `recover` reports restored intent
and registered startup, not a fabricated comparison with the pre-crash model.

Volume available bytes are sampled before and after a successful round trip. This
net delta includes unrelated machine activity. The OS may retain its expanded
pagefile despite the released memory: zero/negative gain is a bounded result, not
capacity recovery. Always restore serving; do not extend the window indefinitely.
No pagefile or unrelated process is touched.

Shared-code receipt: ServingExpectation, owned exit confirmation and one-shot
restoration launch moved from container_maintenance.rs into maintenance_serving.rs;
both maintenance callers now use that owner. Existing container journal/placement
semantics remain unchanged. New Windows CLI CI cases cover strict bounded parsing
and the real composition's failure matrix. The existing custody fixture also checks
partial-disable recovery routing, legacy journal retention, and combined errors;
it injects restoration results rather than mutating OS tasks. Existing lifecycle tests retain task
identity, recovery and durable-drain contracts. Local native build and live
maintenance were not run on the serving node. CI and installed acceptance pending.

Windows CI selection is explicit: PR #5302 carries the existing workflow's `windows`
label. Its initial unlabeled run skipped both Windows jobs, so the aggregate wrapper
was not platform validation. The subsequent push with that label must execute the
actual core CLI and lifecycle fixtures before merge; a skipped wrapper is insufficient.

The labeled Windows run 39895257075 passed the standalone lifecycle job, then
failed core compilation with E0433: the engine's Windows retirement fixture still
called core's serving-daemon test module after the crate split. The correction
moves that existing real-child scenario into the engine's opt-in test-fixtures
surface and passes the core transaction as a callback from a Windows core test.
All original preparation/retirement/restoration assertions remain; the Windows
job runs the exact connected test and requires `1 passed`, rather than relying on
compilation or silently dropping it. Fixture-only tempfile/bin selection remain
behind test configuration; production dependency layering is unchanged. Locked
metadata and diff checks passed locally without compiling beside serving. The
corrected Windows execution and installed maintenance remain pending.

The next Windows run 39897438958 compiled the engine fixture but exposed E0624
in the preserved core callback: its generation-exit observation was still
crate-private after the split. A test-fixtures-only observer now delegates to the
same engine method; no production method visibility changes. The remaining core
callback calls were inspected against the public engine/resource interfaces; the
compiler reported only this access error. Raw failure is retained as
`b705-windows-42eb-failure.log`. Unix and standalone lifecycle passed on 42eb;
connected Windows execution still requires the corrected CI run.

The connected Windows test then executed (4.64s) and exposed a stale HTTP fixture:
its checkpoint server had only POST `/slots`, while the existing checkpoint owner
now discovers GET `/props` and verifies `total_slots` before retiring anything.
The fixture now reports its actual four-slot pool through that endpoint, explicitly
modeling the supported legacy engine without the optional size protocol. No
admission guard is bypassed. Early caller failures now retain their actual Result
in the assertion diagnostic. This is a source-identified fixture omission; the
corrected end-to-end Windows result remains pending, not inferred from source.

The final #5302 head 5253446a8 passed run39905654311: Windows core35m56,
including the connected restoration test (1/1,6.73s), Unix33m9 and standalone
Windows4m18. It merged as315b8624996b. Published Windows artifact39909266183
was checksum-verified (258d79a2570eb2c66f1ffeb2585a061811a10675517210a446f295e70fd3415a).
Actual published `continuum maintenance 60` refused before drain: real Scheduler
COM XML omits Settings/Enabled when true, while our reader demanded that node.
Deploy was immediately restarted through Task Scheduler; Core and Deploy are
Running, with core10620/engine4280 untouched. No capacity recovery is claimed.

The shared reader now excludes the optional Enabled element from registration
identity, preserving raw XML, authoritative COM Enabled, SDDL and every other
registration field. The existing Windows fixture now uses real unregistered COM
task definitions, retaining interrupted-disable/recovery assertions. A direct PS5
execution of the production reader against those definitions passed default true,
false, reenable and registration-drift checks; no registered task was mutated.
Raw receipts: b705-installed-maintenance.log, b705-core-task.xml and
b705-scheduler-default-proof.txt in the team-proof state directory. Native CI and
fixed published maintenance execution remain required. Older pending journals
with the former normalized identity fail closed; this is not transparent journal
migration. No pending journal was found at the installed root during inspection.
