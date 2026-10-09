# Installed service handoff boundary

Card `3db61c8b-643b-4ad5-9982-28fa8684a1ee` follows the actual failed
`3c170a01b` public `ResumePrepared` activation. Preparation and native embedding
had passed; that did not prove the supervised handoff.

Four connected defects were repaired; review also identified a recovery blocker:

- The shared Windows process launcher canonicalized PowerShell to a `\\?\`
  executable spelling. A real child using that spelling failed before its script
  with `System.Net.ServicePointManager` initialization failure (exit -65536),
  matching `service-bootstrap.log`. Ordinary spelling succeeded. The launcher
  now retains canonical file identity but passes verified ordinary drive/UNC
  spelling to Windows. The existing owned-process fixture executes the real
  PowerShell initialization through this boundary, then retains its tree cleanup
  assertions.
- Engine preparation treated the binary working directory as the repository;
  recovery used an unrelated tracked main checkout. `Release::installer_root`
  now owns registered installer-root resolution and required module presence.
  Installed handoff validates it before stopping; recovery uses the captured
  release. Explicit source installation retains its chosen developer checkout.
  The existing fixture loads the real engine/recovery modules in a fresh shell
  from a different directory with a stale tracked-root environment.
- Published engine bytes were staged while an obsolete convergence stamp
  remained. New engine receipts hash the stamp alongside application inputs;
  the shared publisher writes it before receipt capture, copies exact bytes,
  and validates its revision/backend. The Rust launch verifier pins declared
  stamp bytes too; older receipts retain their original membership contract.
  Explicit prebuilt handoff verifies and promotes its registered engine through
  the existing preparation owner after staging. Drift or failed promotion refuses
  before stop and cannot fall through to a source build. The developer path
  remains explicit.
- An existing protocol-2 protected bootstrap was reused indefinitely, so an
  ordinary core update could not install the launcher correction. Protocol 3
  migrates that bootstrap into a protected sibling generation named by its
  validated CLI hash. The prior image remains untouched. Compatible protocol-3
  generations remain reusable for ordinary unelevated core updates; upgrading
  protected authority uses the existing elevated provisioning transaction.
  Legacy descriptors remain readable, and arbitrary generation paths refuse.

Recovery review found that restoring only the previous core receipt leaves the
new engine selected. A previous core can reject the new stamp-bearing engine
receipt, and ordinary failed-engine rollback is suppressed during a deployment.
The handoff must restore its actual captured prior engine selection through the
existing selection owner before restarting the previous core, refusing any newer
selection. `engine_slots::HandoffSelection` captures that actual selection before
promotion; preparation, receipt and stop failures unwind it, and supervised
recovery restores it before restoring the prior core. An absent prior selection
stays absent. Changed supervisor descriptors or engine selections refuse recovery.
The existing promotion fixture covers no-op preparation failure, retry, actual
prior selection, changed current and changed previous; final validation is pending.

Validation in progress: existing Windows PS5.1 service fixture passes 36 groups,
including published drift/stamp/promotion failures without source fallback.
Existing lifecycle tests pass 71/71, including actual corrected PowerShell
initialization through the shared owned-process boundary. Strict lifecycle
Clippy passes. The pre-rollback core CLI check passes (48.10 s), and the updated shared
receipt fixtures pass 2/2. The extended engine receipt regression passes 1/1
(0.04 s; 8m22s build). The final rollback delta still needs its focused run and
CLI check. The original failing/successful PowerShell comparison was observed
in reviewer tool output; the corrected boundary is exercised by the retained
owned-process regression, not a fabricated before/after log.

After the bootstrap-generation change, the full lifecycle suite passes 71/71
(10.86 s), strict all-target Clippy passes, and source hygiene passes 28/28.
These use the existing shared Cargo cache and retained Windows fixture owners.

No running service, lane, task, active receipt, or operator state was manually
altered. A new published candidate and actual supervised acceptance are still
required; this receipt does not claim recovery or authorize a repeated activation.
