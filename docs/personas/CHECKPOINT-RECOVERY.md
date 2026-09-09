# Persona checkpoint recovery

A resident's working memory and per-room own-speech rings are stored in the
native user data directory at `.continuum/personas/<persona-uuid>/volatile.json`.
Resident registration distinguishes an absent checkpoint from a failed read or
invalid schema. Only absence starts a fresh working memory. A load error refuses
that registration, reports the affected slot, and leaves an existing registered
resident intact; healthy sibling slots can still start.

Older cores could write this file beneath their working directory when `HOME`
was absent. They also lacked an acknowledged final checkpoint on some shutdown
paths. A newer core cannot retroactively flush those older processes. Their
periodic checkpoint is a recoverable snapshot, not proof of their final turn.

## Select one legacy snapshot explicitly

The installed CLI exposes this recovery operation without needing a running
core or a repository checkout:

```text
continuum checkpoint inspect --source <legacy-persona-directory>/volatile.json --persona-id <persona-uuid> --plan <new-plan-file>
```

The source's parent directory must match the declared UUID. The legacy payload
does not embed that UUID or a process-lifetime identity, so this is an explicit
operator declaration, not authenticated provenance. Inspection reports source
and destination paths, exact byte hashes, and snapshot summaries. It changes no
checkpoint and creates the requested plan file exclusively. Put that metadata
file outside both Persona stores, using a local path on Windows. Network plan
paths are refused there because a share can alias the native store through a
different path namespace. Reserved checkpoint filenames cannot name a plan.

Choose the intended snapshot from its provenance and work history. Sequence
numbers and modification times from different lifetimes do not establish which
mind is newer; recovery never chooses a winner or merges memories by those
numbers. Both selected and existing destination files must decode under the
current storage schema. An incompatible or corrupt destination is preserved and
refused; this command is not a corruption repair tool.

## Apply while cores are stopped

Stop the core and any automatic launcher that could restart a legacy binary.
If the source changed since inspection, inspect again into a new plan file.
Then apply the exact plan:

```text
continuum checkpoint adopt --plan <plan-file> --legacy-writers-stopped
```

The flag asserts the operational precondition for old binaries that cannot honor
the new file lock. It does not bypass process checks. The CLI refuses a running
core, a live PID-file process, unreadable process evidence, or an unresolved
truncated core process name. Adoption rechecks this condition under the same
per-persona OS lock used by checkpoint loading and saving, including immediately
before replacement. New cores loading that persona participate in that lock.

Recovery preserves the exact selected source bytes and previous destination in
`.checkpoint-adoptions/<plan-hash>/` beside the destination. Complete archives
and an intent record precede replacement; a receipt records the result. The live
file has a separate inode from its archives. Repeating the same plan verifies
the preserved evidence and current bytes, and can finish a receipt interrupted
after replacement. Changed inputs or conflicting evidence cause refusal.

Files are flushed before publication and parent directories are synchronized on
Unix. Filesystems without the required local hard-link/rename operations are
refused. On Windows, the receipt does not promise directory-journal persistence
across power loss. In every case it explicitly reports
`legacy_final_flush_acknowledged: false`.

After normal startup, verify the running build and inspect the resident's
restored memory and actual inference requests. Successful adoption proves which
bytes were selected and preserved; it does not prove the last legacy turn was
saved, a teammate message reached an inference request, or learning improved.
