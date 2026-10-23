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
