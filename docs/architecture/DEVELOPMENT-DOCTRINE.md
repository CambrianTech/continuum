# Development doctrine: architecture is part of the work

Canonical engineering policy, Joel, 2026-10-30. Applies across projects, to authors
and reviewers, at every stage. It governs implementation choices now, not a later
cleanup phase. Existing detailed guides supply mechanisms; they cannot authorize an
exception to these contracts.

## Optimize the whole job

Speed means the time and resources needed to reach a correct, usable system and
keep changing it safely. Time to the first patch, benchmark score, code volume,
and a green build are insufficient measures. Count review, integration, repeated
compilation, operational repair and future changes as development cost.

Design ownership, extensibility and concurrency before implementing the behavior.
They are part of the feature. Do not defer them as polish or compensate for their
absence by buying more compute, adding retries or hiring another loop to watch it.
Measure actual bottlenecks before changing resource allocations. Hardware capability
is a placement input, not permission for wasteful software.

## One mechanism, one owner

Find the existing owner before adding behavior. If several paths implement the same
mechanism, consolidate them and remove the copies. A wrapper around competing owners
is not consolidation. A second queue, scheduler, task registry, completion channel or
retry policy requires demonstrating that it represents a distinct concern.

Separate concerns into crates with narrow public APIs and enforced dependency
direction. Keep implementation state private. Modules organize a crate; they do not
replace a compiler-enforced boundary when independent ownership is needed.

Use shared implementations behind traits or interfaces. Implementations should
supply their specific behavior; the shared owner supplies lifecycle, cancellation,
admission, resource accounting and delivery semantics where those contracts apply.
Choose abstractions from real invariants and extension needs, not speculative
frameworks or inheritance depth. The test is whether changing a shared rule requires
changing one owner, rather than editing each implementation.

## Refinement is continuous

Every existing file is a candidate for simplification. Refinement is ordinary engineering
work, not a separate cleanup season. Whenever a concern is touched, examine its callers,
repeated logic, unnecessary states, weak boundaries and obsolete code. Improve the owner
and delete what the clearer contract makes unnecessary. Keep each change coherent and
reviewable; coordinate cross-owner changes instead of leaving competing implementations.

Compression means fewer concepts and less duplicated machinery, not shorter identifiers,
dense syntax or hiding complexity behind wrappers. Measure whether the same behavior and
stronger guarantees require less code and fewer independent decisions. Do not accumulate
known structural defects for a future rewrite: reduce them in small validated steps while
building capabilities. An untouched defect remains tracked, not silently called acceptable.

Apply the same continuous discipline to resident learning through the existing learning
modes, not a replacement generic batch loop: low-duty-cycle conscious learning during
activity, daydreaming between demands, and deeper dojo/coursework and dreaming work.
The [one-resident patient/doctor/dream design](ONE-RESIDENT-MODEL-PATIENT-DOCTOR-DREAM.md)
owns stable/shadow adapters, shared-base coursework, adoption and the governor's stage
mix (serve-only, serve+dream, dream-heavy). [Event mind](EVENT-MIND.md) owns the surprise
and settled-experience signals; [dream consolidation](../cognition/DREAM-CONSOLIDATION.md)
connects memory consolidation with teaching. These are existing contracts to implement
and verify, not new names for independent schedulers.

Capture, training and evaluation use owned queues, backpressure and resource accounting
within that design. Do not make an overnight batch a prerequisite. Serving reads the
stable adapter; optimizer work updates the shadow. Adoption requires its evidence and
atomic transition, and subsequent useful work must retain the improvement after restart.
In-place learning and warm state reuse are capabilities to prove, not assumptions. The
shared governor allocates headroom without starving useful work.

## Zero backward compatibility

There is no backward-compatibility requirement. This is the first beta design.
Replace the contract, update every caller, and delete the obsolete path in the same
change. No compatibility shims, dual APIs, old-format readers, optional old behavior,
or fallback implementations retained for hypothetical users.

Durable identity, trust, data, jobs and unfinished owner work remain protected.
Explicitly transform durable state when a new contract requires it; do not retain
parallel runtime contracts or discard state to make a test pass. Coordinate development
maintenance and restore through the supported lifecycle.

## Firm contracts; callers comply

Represent valid states and messages with strong types. Make ownership, authority,
required inputs, failure outcomes and effects explicit. Reject invalid input with a
typed failure. Fix the caller; do not weaken the callee to accept ambiguous input.

No silent fallbacks, guessed defaults, substituted identities, swallowed errors,
unchecked unwraps, or panic-as-normal-control-flow. A missing dependency or failed
operation must not be reported as empty success. Legitimate recovery is an explicit
state transition in the same owner, with a declared policy and observable outcome;
it cannot switch to a weaker contract or hide lost work.

Route messages by authenticated headers and typed envelopes. AIRC transports opaque
payloads; it must not scan bodies to discover a route or guess a message schema. Only
the selected consumer decodes its typed payload. Reject disagreement between metadata
and any duplicated payload fields; never choose whichever happens to parse.

## Concurrency and performance are structural

Long work has owned lifetime, a handle, cancellation and a terminal outcome. Its
completion wakes its owner. Do not spend inference turns polling for completion.
Define queue bounds, backpressure, fairness, shutdown, deadlines and restart behavior
before spawning work. No unowned tasks or per-handler copies of lifecycle machinery.

Keep blocking work off asynchronous execution paths. Keep expensive work out of hot
routing paths; consume owner-published state where appropriate. Share scheduling and
resource accounting across implementations instead of creating private budgets.
Measure latency distributions, throughput, memory, queue behavior and useful completed
work on the actual consumers. More activity is not more progress.

## Completion includes deletion and proof

Before implementation, name the invariant, owning crate, public contract, callers,
obsolete paths to delete, and smallest meaningful validator. Keep that account short
and concrete. Reviewers check those decisions against the code, not the description.

A change is incomplete while obsolete callers or duplicate mechanisms remain. Tests
for a deleted contract leave with it; tests for the replacement must demonstrate its
real guarantees. Do not preserve a wrong behavior merely because its test was green.

Use types and dependency boundaries first, then focused behavioral tests and ratchets.
Exercise failure, cancellation, concurrency and restart paths relevant to the contract.
Prove that important regressions are detected; do not use tests that merely restate the
implementation. Ratchet budgets never grow to accommodate a patch.

Review and CI refer to exact commits. Distinguish source review, executed tests,
installed revision, running revision and behavior observed by real consumers. A
transport acknowledgement is not proof that a mind received and used a result. A
trained artifact is not proof of retained learning. State the remaining proof honestly.

## Review questions

- Does one owner now implement this mechanism, with fewer independent states and paths?
- Can a new implementation reuse the contract without copying its machinery?
- Are invalid states rejected at the boundary, and have all callers been updated?
- What obsolete implementation was deleted? What remains, and why is it still necessary?
- Are concurrency and resource costs explicit and measured where this change affects them?
- Which test fails if the claimed guarantee is broken, and what was verified live?

Reject violations rather than creating a follow-up that makes them permanent.
Line count is evidence of simplification, not its target: preserve necessary behavior
while reducing the concepts and mechanisms required to express it.

## Detailed guides

- [Microkernel decomposition](MICROKERNEL-DECOMPOSITION.md): crate boundaries and dependency direction.
- [Concurrency style guide](CONCURRENCY-STYLE-GUIDE.md): shared concurrent mechanisms.
- [Proving the doctrine](PROVING-THE-DOCTRINE.md): architectural evidence and validators.

When older guidance conflicts with this doctrine, update it rather than maintain two
rules. Historical inventories are evidence of past state, not current acceptance.
