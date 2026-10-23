# Prompt capacity refusal and retained input custody

Card: 65761cae. Isolated source draft; no installed acceptance claim.

Before: required prompt capacity was rendered as a contribution fault, and zero-generation settlement could classify it as lane starvation. Retained input retried on the ordinary beat, preventing an unchanged refusal from becoming a stable waiting obligation.

Shared owners now carry the refusal as a typed workspace contribution through settlement and the turn outcome. The settlement boundary recognizes it before zero-generation pace classification, preserving earlier actions, metrics and generation receipts. Replay is permitted only when no earlier action was dispatched. PendingInput retains original input, engram and attention; new input and Stop remain selectable.

CapacityReadiness records the generation target, requested model and owner window before assembly. Binding publication, adapter window feedback and independent capacity gossip watches notify existing wake selection; allocator notify_one remains its own notification. Notifications trigger semantic comparisons, not model turns. Provider refitting retains both the dispatch observation and the smaller fitted bound: stale nominal capacity cannot spin, while a different owner window above that bound permits another assembly. Same numeric capacity made valid by an unobserved backend generation cannot be proven by this numeric witness; a future authoritative generation identifier would be needed, not a timestamp wake.

Self refusals are scoped to room. Typed board facts are observed by the existing inbox resync before non-chat trigger filtering. The bounded witness excludes lease/Other traffic and includes durable submission/claim IDs; repeated facts stay quiet. Tool schemas are immutable on a resident LlmDeliberationFaculty (with_tools consumes the faculty); this patch does not invent a schema mutation feed. Faculty replacement remains a new resident lifecycle.

Quiescence retains ready custody and uses the existing beat to recheck lease release without retrying cognition. Ordinary directed inputs remain reachable.

Regression source covers typed full settlement, refusal after prior work with metrics retained, independent binding/gossip/remote-adapter consumers, preassembly growth, provider-refit growth, retained input/engram, unrelated rooms, actual wake selection for Stop/new input/quiescence, and typed board changes before trigger filtering including distinct submissions.

Validation so far: all 40 touched/new Rust files parse through rustfmt without file mutation; git diff --check passes. No Cargo checks or regression execution locally because serving/compiler ownership is reserved. CI execution and independent final source approval remain required. No persona/model binding configuration, active jobs or runtime state changed.

## Follow-up for Cormac's source review

`capacity_park_resumes_when_local_1_5b_routes_to_larger_remote_seat` now drives the real PlacementSwitch through FallHome/ReturnRemote, captures the actual 32768 local generation target, and verifies that the 65536 remote target releases readiness even when the wait future has not yet been polled. Stub transport panics if inference is dispatched; this is a source regression, not an observed M5 inference run.

`persona.capacity.transition` carries persona, room, refusal UUID, required/budget tokens, phase and reason. A shared per-refusal atomic state permits one parked and one terminal disposition receipt across cloned projections. Reasons distinguish binding replacement, placement target change, window change, relevant input and Stop. Bound-adapter pointer replacement is explicitly retry evidence even when the forwarded target is unchanged: a new bound provider may change request handling. Input-resumed means eligible to assemble again, not dispatched or successfully answered. Normal loop exit emits stopped/stop for still-parked refusals; task cancellation/drop is not covered. Readiness checks and unchanged beats emit no receipt. The existing receipt test checks clone deduplication and distinct next refusals.

Untouched settle_to_outcome formatting was restored. Follow-up parses without compilation; independent authorization_review source approval is conditional on final post-5308 integration and actual CI execution. No installed acceptance claim.

## First CI corrections

Unix job120373496903 failed to compile one retained-input regression because its assertion still used the superseded single-input wake_engram accessor. It now checks wake_engram_for(&original), preserving the original engram assertion. Hygiene identified the new remote capacity test file as production because it lacked the inner test-only marker; its existing parent cfg(test) gate is now repeated at the file boundary, with unchanged test registration. New production witness-lock and new-message attention unwrap sites now document their actual fail-loud/derivation contracts. No baseline, assertion or execution gate was relaxed. Raw logs: state/team-proof-20260921/5309-unix-job.log and 5309-hygiene-failure.log. These corrections require a new CI run; no local compilation or installed acceptance is claimed.
