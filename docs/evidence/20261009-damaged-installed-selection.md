# Recovery of an engine-damaged installed selection

The failed normal Windows installation left a valid Prepared release while
Active and Previous still sealed the older bytes of the same engine slot.
Preparation had reused that slot. The strict receipt guard correctly refused
the old selection; its generic error incorrectly suggested Prepared was damaged.
Slot prevention is a separate BigMama-owned fix.

`windows-prepared.ps1` remains the single receipt owner. Its shared reader now
supplies strict validation and a narrow recovery diagnosis: only engine hash
damage matching the path and verified bytes of the complete saved Prepared
release qualifies. All other schema, owner, layout, hash-set or payload errors
refuse. Ordinary receipt reads still reject damaged selections.

The public installer routes this recognized condition to its existing saved
preparation resume before restaging. Registration validates the candidate and
protected bootstrap normally. Recovery then requires stopped, known task states
and no core process; rechecks registration and receipt bytes; pins and rehashes
the candidate; archives original damaged receipts using flushed temporary files
and atomic publication; removes invalid Previous eligibility; and atomically
selects the candidate. Valid Previous remains byte-for-byte unchanged. Invalid
Active is never resealed or rotated into rollback. No historical claim that a
release never served is inferred from this damage.

The install lease does not exclude scheduled starts. Old Active is launch-invalid
under the supervisor's required full receipt hash validation. A scheduler start
after the atomic switch may legitimately start the validated candidate. Observed
running/queued tasks or a changed registration refuse recovery.

Validation: the existing Windows PowerShell 5.1 service fixture passed all 37
groups. The extended scenario covers ordinary strict refusal, unrelated damage,
archive failure, scheduled-start refusal during archival, a real failed atomic
replacement while Active is held open, retry after partial retirement, original
evidence retention, and unchanged valid rollback. The existing public installer
subprocess scenario verifies normal `-Grid` selects saved Prepared before loading
the provisioning/build module; registration and handoff are mocked at their
existing boundary. Parser and `git diff --check` passed. Raw local test output:
`C:/Users/joelt/.continuum/state/team-proof-20260921/20261009-damaged-selection-tests.log`.

No installed receipts, slots, tasks or services were changed for this validation.
Actual installed recovery and successful Kimi operation remain unproven.

## Legacy Previous omission found by real read-only diagnosis

After #4889, BigMama's read-only check of the installed receipts found that
Previous retained only the four original hashes, predating runtime DLL sealing.
The original regression used a modern Previous and missed this exact case; the
reader refused its hash set before engine-only recovery could be diagnosed.

The same receipt reader now recognizes that legacy shape only for the recovery
diagnostic of Previous. All four original field names, hash formats and bytes
are checked; exactly the engine must differ, and the existing recovery owner
still requires its path and actual bytes to match the fully sealed Prepared
candidate. This proves engine-only damage among historically sealed fields, not
that previously unsealed DLLs were unchanged. The legacy receipt is archived and
removed from rollback eligibility.
Ordinary Previous/Active reads remain strict. An incomplete legacy receipt with
no engine damage is refused, not declared a valid rollback.

The existing recovery scenario now runs archival failure, partial commit and
retry against this exact four-hash Previous. It also checks strict ordinary
rollback refusal and refusal of an incomplete undamaged legacy Previous; the
modern valid Previous preservation scenario remains intact. No runtime receipt
was edited to make the check pass. Hosted checks and BigMama's real read-only
diagnosis remain separate from installed recovery. The complete existing PS5.1
fixture passed 38 groups on the integrated canary base; raw local output is
`20261009-legacy-previous-tests.log` in the team-proof directory.
