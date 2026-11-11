# One act, end to end

**Status:** draft for agreement by Fable, Codex and Cormac (2026-11-04). Nothing new merges until
the three sign the "Agreed" line at the bottom; every open branch is then fit to this or closed.

**Revision 2026-11-06:** Joel directs refinement of this existing design before further runtime
implementation: centralized algorithms, explicit interfaces, and fewer decisions at the leaves.
Sections 4, 5e and Proof describe the target, not a claim that it is implemented. Historical
receipts below remain evidence, not current operating instructions. Current code is the engineering
baseline; deployment/version checks are not productive acceptance. No activity-based rollback.

**Why this exists.** Joel, 2026-11-04: "There is no coherent design anymore." On that day the M5's
four residents went from writing to dark and back four times, every part green. The docs disagree
with each other and the code follows a third version neither describes (inventory below). This
document is NOT new doctrine. It follows ONE act of ONE resident through the system, names the one
owner and the one invariant at each stage, rules on every contradiction (which existing contract
wins), and states how the whole is PROVEN before any of it goes live.

**The goal it serves** (CLAUDE.md, the path): residents who code at pace, many at once, on any
hardware. The measured obstacle is not the model. It is that the substrate makes her re-read: every
token in her context is paid again in PREFILL whenever the engine's copy is lost, and prefill is the
one resource that does not scale with concurrency (measured below).

## The numbers this design is held to (measured 2026-11-04)

| Fact | Value | Receipt |
|---|---|---|
| M5 memory | 68.7 GB (M5 Pro), not 128 | `sysctl hw.memsize` |
| Model weights (Qwen3.6-35B-A3B) | 20.4 GB, wired | lane footprint |
| Reclaimable at launch | 21-24 GB with the old engine still wired; 45.6 GB after its release | vm_stat 20:56Z, 21:00Z |
| Launch pools the same build got | 33,040 / 50,275 / 88,425 / 125,640 / 413,625 / 654,336 tokens | engine `-c`, by launch timing |
| Prefill, IntelMac CPU, matched | 68.3 tok/s at 1 stream, 68.0 at 2, 63.2 at 4 (server ~1,070% CPU): does NOT scale | Cormac's bench, 21:4xZ; raw in `evidence/knee-20261104/` |
| Decode, IntelMac CPU, matched | 19.9 / 32.8 / 43.6 / 51.1 tok/s aggregate at 1/2/4/9: scales | same |
| Prefill by prompt size, M5 (one engine, flash-attn, KV q4_0, ubatch 2048) | 399-9,371 tokens: 415-1,023 tok/s; 13-24k: 135-348; 141,739 tokens: 148 tok/s (~16 min for one act) | llama-server prompt eval lines, 2026-11-04 |
| Cold act, M5 | Atlas 76,114 prompt tokens, 0 cached: 318 s prefill | 21:37Z |
| Warm acts, M5 | Aiko ~20k prompts: 28 acts in 20 min; Aris ~89k prompts: 9 acts in 30 min | 20:15-20:35Z, 18:49-19:22Z |
| A whole-pool grant to one mind | pool 125,696 -> {Aiko 114,592}, three dark 20 min | 20:15-20:35Z |
| An admission hold nobody released | 16 min, zero generations | 18:00:46-18:16Z |
| Acts that wrote | 2 of ~100 all day | persona.act.observed wrote=true |

**What the numbers say.** A resident's pace is set by how much she must prefill per act, and how
often the engine loses what she had. Big contexts are the slowdown, not the cure. Admission must
count prefill work, not lanes.

## The act, stage by stage

Each stage: the ONE owner, its invariant, and what wins where docs or code disagree.

### 1. Her state (what she is doing)

- **Owner:** the activity's typed state and each command's precondition
  (`continuum-rooms/work_preconditions.rs`, branch `fable/state-driven-surface`), moving into
  `airc-work` (Cormac, card be8c4d78) so the board refuses what no precondition allows.
- **Invariant:** one predicate per command decides BOTH whether she sees the verb and whether it
  executes; execution revalidates against current state at the mutation boundary and returns her
  refreshed moves on a stale view; decisions read recorded event time, never the wall clock.
- **Rules:** STATE-DRIVEN-SURFACE (precondition revision) wins. Per-command `Disclosure` becomes data
  only for commands no activity covers; the room recipe declares the activity, not a tool list.
  `ToolsEpoch` switches at a state change (goal 10's "a switch is a cached prefix"), not only when
  the window cannot hold it.

### 2. Her view (what she sees, and how big it is)

- **Owner:** `continuum-context` (one codec), its ONE allocation run ONCE per act.
- **Invariant (goal 7, unchanged, now enforced):** the newest complete interaction is whole; her
  required working set (her held card's state item, the region of the file she is editing, the
  result she is acting on) is whole; everything older and reconstructible is a HANDLE at a level
  (`levels::summarize`) she can re-expand. A read returns the selection she asked for; the view, not
  the command, decides what stays verbatim.
- **What this replaces:** today a read without a range gets `chars / reads` of headroom
  (`act_observe/page_budget.rs:33`: division by a count) and the newest result can fall to a bare
  pointer (`prompt/newest_result.rs:106`); side budgets are computed outside the allocation
  (`prompt/view.rs:199-235`) and the allocator runs three times per act. All of that goes.
- **Size is a consequence, not a target.** Her prompt is head + working set + newest interaction +
  handles. Today's evidence says that is ~20k tokens for a coding act; the 76-89k prompts were
  history kept verbatim. No fixed number: the validator reports prompt tokens per act and the cold
  re-prefill seconds they imply at the measured rate.
- **Rules:** MIND-CONTEXT's "flex moves into continuum-context as the one allocator" wins over
  ONE-BUDGET 3b's "deleted for cambrian-flex" (the code exists; a replacement must replace it whole).
  HER-TOOLS §Answer ("the view is the only sizer") wins over page_budget.

### 3. Her demand (how much residency she asks for)

- **Owner:** `working_set` (need = her FLOOR: head + required + newest interaction; want = floor +
  working set).
- **Invariant:** demand is what her CURRENT view needs, never her history. An overflow is first
  REDUCIBLE: eligible older items collapse to handles (stage 2). Only if the required material and
  the newest complete interaction still exceed the grant is it IRREDUCIBLE working-set growth, and
  then she requests admission-backed growth, placement elsewhere, or waits, with a typed reason
  (Codex: "never a bigger grant" would be another hard-coded policy, as "automatically grow" was).
  A censored observation (`Observed::AtLeast`, #5773) is the honest measurement either way; it does
  not by itself authorize residency.
- **Correction of 2026-11-04:** #5773 raised the WISH to a censored bound; that is reverted (the
  typed measurement stays). This is why one mind could take a whole pool.

### 4. Admission (who runs now)

- **Owner:** ONE admission decision per engine (SERVING-ENGINE §1c wins: "admission is memory,
  once"; one dispatcher). Today three gates run in series (`resource_admission/lanes.rs`,
  `turn_admission.rs`, `window_grant`); they collapse into one.
- **Three resources, decided together (Codex, Cormac's bench):** (a) sequence RESIDENCY in bytes
  (her KV), (b) PREFILL work (serial on this hardware: 68 tok/s at 1 or 2 streams), (c) DECODE work
  (scales with streams). A lane count is none of these; the semaphores sized from `--parallel` go.
- **Fairness (goal 9):** consumes identified completion and verified release events, never a polled
  in_flight count. An eligible waiter owed service gets the next *feasible* opportunity; release of
  one turn does not imply that sufficient compatible residency was released. State the service
  bound in opportunities and measured non-preemptible work. Eventual service assumes finite
  preceding work, feasible demand, and a functioning backend; an infeasible request gets an explicit
  placement/shortfall outcome, not a promise of a universal wall-clock deadline.
- **A grant is permission, not bytes (Codex):** rotating a seat asks the page custodian to surrender
  the holder's KV; her residency stays charged until the page-out (or erase, or incarnation fence)
  is verified; only then is the capacity published.
- **Priority:** activity temperature (goal 9). Until it is measured, Directed/Work/Ambient is its
  coarse proxy, named as such in code.
- **Rules:** INFERENCE-LANES-REALISTIC, INFERENCE-SCHEDULING-AND-SCARCITY's slot pools,
  KV-CACHE-ECONOMY's `-c = window x lanes`, MINDS-SEATS "a seat is a lane", CONTEXT-IS-A-CAPABILITY-AXIS's
  lane count: SUPERSEDED (marked so, removed from CLAUDE.md's "read first").

### 4a. One exact dispatch decision

Joel's direction: use evidence to correct the current architecture with formal, robust algorithms and centralized logic. Deployment/version activity is not progress toward useful work. Do not stack independent retry patches. This section supersedes any interpretation of the provisional reply-room code as an accepted design or performance fix.

Evidence: Kimi's identical prompt was rejected at predicted cap 5857, then accepted at 5290 after another admission wait. Exact prompt P=27221, physical window W=32512, required reply minimum m=4534. The quote took 67ms; corrective admission waits took 156.5s and 24.6s. This demonstrates disagreement between a predictive render decision and exact dispatch, not a defect in the safety inequality. Cold prefill and pinned-page waits are separate measured costs; this correction makes no claim to solve them.

Owner: continuum-engine's existing dispatch path owns final feasibility and the executable request. The existing window_grant owner retains atomic reservation and lifetime; the existing scheduler retains fairness and queue custody. Do not introduce another scheduler, reservation registry, or retry coordinator. Cognition supplies intent and required reply room, and may estimate prompt size for rendering. Estimates never authorize dispatch or masquerade as the sent cap.

Contract and algorithm:
1. Submit an immutable prompt plus explicit output intent: Fixed(r), or UseAdmittedRoom(required minimum). The admitted window supplies the finite upper bound; a predicted cap is not an authority ceiling. Reasoning allowance remains an explicit component of the required reply claim.
2. After existing endpoint/page admission, bind the attempt to the selected engine incarnation, model/template and current grant lease. Obtain the physical sequence capacity from that same owner-bound context. A missing capacity or mismatched incarnation is a typed refusal, never a guessed window.
3. Count that finalized prompt on that engine. Let W be the intersection of physical sequence capacity and admitted lease capacity. For Fixed, choose R=r and refuse unless P+R<W. For UseAdmittedRoom, maximize R subject to m<=R and P+R<W. Equivalently R=W-P-1, using checked arithmetic; refuse if R<m. There is no tuning constant or relaxed floor.
4. The grant owner atomically validates/reserves the exact attempt. Only a successful reservation constructs a private executable dispatch value containing prompt identity, engine identity, lease identity, P, W and R. Invalidation before reservation requires a new attempt and quote; it cannot reuse the predecessor's count. Preserve cancellation and terminal settlement in the existing owner.
5. Build the wire request and its sent receipt from that executable value. Transport cannot independently recalculate its cap. Cognition classification, allocation accounting and capture consume that same receipt. A resolution receipt alone is not proof that a request was sent or completed.
6. Re-render only when the required claim is actually infeasible. Prediction error that leaves the required claim feasible resolves inside the admitted attempt, without a capacity-repair round trip through cognition. Transport recovery after a proven pre-acceptance failure re-enters this same algorithm; ambiguous post-acceptance outcomes do not authorize duplicate generation.

Required consolidation: remove the competing estimate-derived final-cap decision for adaptive acts and its resize-only retry path; retain estimation only for prompt preparation. Combine physical/admitted capacity checks into the dispatch decision rather than adding an extra clamp beside existing checks. Unsupported providers must explicitly reject this authority or implement the same contract. Fixed-call intent is a real distinct contract, not a compatibility fallback. The optional response receipt in local f420b8a490 is provisional plumbing; review it against explicit provider capability and actual sent-state semantics before integration.

Proof obligations before deployment: boundary/property tests establish feasibility, maximality within authority, monotonic capacity behavior and arithmetic safety; actual HTTP tests establish one quote and one generation for the measured Kimi case, unchanged prompt, and identical wire/receipt R. State-transition tests exercise engine replacement during quote, grant revocation, cancellation before and after reservation, quote failure, insufficient minimum, fixed requests, and ambiguous send failure. Every invalid transition must prevent unauthorized POST and release or retain custody according to the existing terminal contract. Then verify the live elimination of resize-only retries and measure useful completed work, queue time, prefill and cache reuse separately. Compilation, deployment and model-reached counts alone do not satisfy acceptance.

#### Contract review against the current implementation

Inspection at local `f420b8a490` identifies the following consolidation boundaries. These are
design requirements for the coherent replacement, not instructions to add another parallel path.

| Current location | Conflict / missing boundary | Required replacement |
|---|---|---|
| Cognition `act_room::request_allowance` | Predicts final act cap from calibrated prompt density | Keep prediction for rendering/feasibility planning; submit the reply claim as intent, not a supposedly sent cap |
| `contribute` assigns `sent_allowance` before generation | Planned value becomes outcome evidence | Obtain sent allowance from the successful dispatch result; failed attempts cannot manufacture a sent receipt |
| `lane_send` checks physical and admitted windows separately | The exact facts are available but no single executable decision owns them | One feasibility result over their intersection, then reserve through the existing grant authority |
| `lane_send` prepares immutable wire bytes before exact resolution | Output intent is already flattened into a guessed `max_tokens` | Separate immutable prompt identity from output controls; serialize the resolved executable request once per authorized attempt |
| `window_grant::reserve_attempt` | Already owns replacement of an unsent reservation | Preserve this owner; prove its release-unsent precondition at every transport retry, do not add another reservation layer |
| `TurnAdmission::_endpoint` / `EndpointGeneration::admit` | Already retains endpoint read admission and the admitted pool's incarnation | Pass this owned capability into exact dispatch; do not replace it with a copied PID, an unrelated watch snapshot, or another lifecycle lock |
| Prototype optional `quoted_reply_room` | `None` can conceal a missing receipt on a path that promised counted admission | A successful counted dispatch must contain its sent receipt; provider-enforced fixed dispatch is a distinct explicit result/capability, not absence interpreted as fallback |

**Resolve the intent before implementation.** Current act-room documentation authorizes an act to
use the room left in its admitted window; the minimum protects the combined think/answer claim.
Thus the predicted 5857 is not an independently authorized maximum. The public contract should
express `Fixed(reply)` versus `UseAdmittedRoom(required_reply)`. The latter resolves
`R=W-P-1` and refuses if `R<required_reply`; the grant supplies the finite upper bound. If a real
caller needs a smaller maximum, it must declare that distinct restriction explicitly and justify
the additional variant. Do not retain the prototype's ceiling merely because it is already coded.
For a genuinely bounded variant, the same solver adds R<=declared maximum; it does not substitute an estimate for that authority.

**Prompt identity must be enforceable.** The quote and POST must share model/template, adapters,
messages, tools, media and every other input affecting tokenization. Output controls may change
only if the backend contract proves they cannot alter the finalized prompt. A backend whose template
depends on those controls must quote the final form or refuse the capability; a token count alone
does not certify identity. Engine incarnation changes invalidate the quote even when URL/model match.

Backend source audit (`core/vendor/llama.cpp` at `0d7dda6aea3531f79adcb22bc35c8a2bb3f514d7`):
`handle_count_tokens` runs `oaicompat_chat_params_parse`, then tokenizes its rendered prompt
(including the multimodal path). The parser constructs template inputs before copying remaining
sampling properties. `max_tokens` is not a template input in this path, while messages, tools,
tool choice, schema/grammar, continuation, reasoning effort and template kwargs are. Freeze those
prompt inputs; resolve only the output limit. The parser explicitly lets `n_predict` override
`max_tokens`, so the executable serializer must own a single canonical output limit and reject
conflicting aliases rather than report a receipt for a cap the backend will override. This source
audit narrows the supported capability; it is not a backend execution test or proof for arbitrary
OpenAI endpoints. Validate the canonical-limit and prompt-identity contracts in the actual backend
adapter before enabling adaptive dispatch.
**Completion is not submission.** The prepared command, accepted send and terminal generation have
different types/states. The sent receipt accompanies the selected attempt; the terminal result links
to it. Serialization and observer consumers do not own a second arithmetic implementation. Capture
must preserve requested intent separately from actual sent controls so training and diagnostics can
reconstruct both without guessing from a nullable field.

Review gates before runtime editing: agree these public types and backend capability semantics;
identify all counted/fixed callers and consumers; specify which resize-only retry branch disappears;
and record how the engine incarnation is fenced across quote, reserve and send. The mock-HTTP
baseline and pure arithmetic tests do not prove those concurrency boundaries.

**Existing concurrency primitives to retain (source audit, 2026-11-06):**
`EndpointGeneration::admit` acquires the endpoint read guard and rechecks readiness;
`TurnAdmission` retains that admission through generation. Controlled lifecycle transitions acquire
the corresponding writer. `window_grant::Reservations::reserve_against` compares the exact lease
under its reservation lock; regrant publication uses that same owner. These are complementary
endpoint-lifetime and capacity invariants, not reasons to add a third gate. The dispatch API must
retain access to both owned capabilities through quote/reserve/send. Passing only incarnation
numbers loses the ability to express that ownership in the type boundary.

Controlled transitions and external engine failure require distinct validation: the read guard
serializes the former but cannot prevent an OS crash or external replacement. On transport recovery,
re-establish current endpoint identity and the matching lease before a new quote/POST. Test same-URL
replacement and late responses explicitly. This audit identifies the existing mechanisms and the
proof needed; it does not claim that the current transport already proves the entire contract.

#### First execution slice: public contract and caller coverage

The first slice implements section 4a inside existing dispatch; it does not simultaneously replace
the scheduling algorithm or alter view-restoration policy. It must replace the competing cap
decision end to end, not land an optional adaptive branch alongside it.

Conceptual public contract (names may follow crate conventions):

- `OutputIntent = Fixed(positive_reply) | UseAdmittedRoom(positive_required_reply)`.
- `GenerationIntent` carries immutable prompt content, output intent and explicit reasoning terms.
  Prompt estimates belong to planning data, not this contract's final sent controls.
- `DispatchSupport` states which intent and admission guarantees a selected provider can enforce.
  Capability is checked before work is submitted and revalidated at the executing owner.
- `AdmittedAttempt` is private, owns/borrows the existing endpoint and reservation capabilities,
  and has a single consuming operation that sends its executable request. It cannot be constructed
  by cognition or deserialized as proof of local custody.
- A successful result carries either `CountedDispatch(sent_receipt)` or an explicitly supported
  `ProviderEnforcedFixed(sent_controls)` result. A requested counted contract can never return the
  latter. Requested intent, actual send and terminal outcome are distinct records tied by identity.
- General generation may explicitly report `Unadmitted`; this is not a third successful admitted
  outcome or a compatibility default. Every serialized response states its dispatch kind. The
  shared admitted-call boundary rejects `Unadmitted`, contradictory fixed controls, and a
  provider-fixed receipt for counted/adaptive authority. Adapter implementations supply behavior;
  this boundary owns validation. Capture preserves the same mandatory dispatch observation,
  while classification and allocation consume its actual sent reply, never an absent-field fallback.

| Caller/provider path audited | First-slice obligation |
|---|---|
| Cognition act through managed local OpenAI adapter | Submit UseAdmittedRoom; exact dispatch resolves and reports cap; no predicted final cap |
| Pass / ordinary fixed generation | Submit Fixed explicitly; retain exact cap or refuse, never silently adapt |
| Managed exact-count endpoint | Implement through the common dispatch owner, including physical/admitted intersection and existing lifetime guards |
| Other OpenAI-compatible endpoint | Do not infer exact-count support from protocol or URL shape; unsupported intent is a typed refusal |
| Anthropic / in-process llama / heuristic fixtures | Explicitly implement supported fixed semantics or refuse unsupported terms; tests declare their capabilities honestly |
| Placement-switch adapter | Resolve target once for admission and dispatch; forward the same intent and support contract |
| AIRC remote adapter and LocalAdapterTransport | Current path forwards think but refuses leases/exact admission. Refuse UseAdmittedRoom until destination capability, wire intent and returned sent receipt are implemented together; never strip terms or claim the test transport proves distributed custody |
| Capture, replay, allocation, stop classification, training observations | Consume actual sent controls and preserve requested intent separately; no render-cap substitution when counted evidence is required |

Concrete deletion: `contribute::room_resized` and its identical-view resize/retry branch disappear
when UseAdmittedRoom is resolved by dispatch. Its `newest_refit_sent`/required-content refit handles
a different condition and remains until that condition is consolidated with the context owner.
The prototype's optional receipt and estimate-derived ceiling are replaced, not maintained as a
compatibility route. Update old tests that assert the deleted retry; keep the baseline reproduction
as recorded evidence and replace its success expectation with one authorized dispatch.

First-slice readiness is a reviewable inventory of those callers, the concrete owned-attempt API,
and validators for the invalidation cases above. Broader fair scheduling work can proceed against
the same operation/receipt interfaces later; it is not permission to stack another priority policy
inside this slice. These contracts are now the implementation target, with distributed adaptive
dispatch explicitly unsupported until its complete owner contract exists.

#### Owned-attempt API and settlement boundary

The implementation belongs to the existing engine dispatch module. Its private stages are
`PreparedPrompt -> QuotedAttempt -> ExecutableAttempt -> InFlightGeneration -> TerminalOutcome`.
These are ownership transitions, not five new daemons or registries. Only immutable observations
are serializable; guards, executable authority and terminal proof cannot be reconstructed from JSON.

| Stage | Owns or borrows | Consuming transition / refusal |
|---|---|---|
| PreparedPrompt | Finalized prompt encoding and explicit output/reasoning intent | Quote only after borrowing the actual `TurnAdmission`; no capacity decision from estimated tokens |
| QuotedAttempt | Prepared prompt, exact count, owner-bound engine/model/template identity, physical capacity, current grant lease | Resolve once; required-content shortfall returns a typed refit requirement; identity/readiness mismatch invalidates the attempt |
| ExecutableAttempt | Same endpoint/page capability plus the grant owner's successful reservation; resolved controls and immutable prompt | `send(self)` is the only POST entry; generates wire and receipt together, so neither caller nor transport can substitute a cap |
| InFlightGeneration | Response/stream handle, endpoint/page admission and reservation, selected attempt identity | Terminal receipt settles through the existing grant owner; cancellation or ambiguous transport failure transfers unresolved custody to the existing endpoint owner |
| TerminalOutcome | Verified terminal identity and immutable sent controls/usage | Releases or retains physical resources according to their own completion domains; wakes dependent work |

API implementation constraints:

- Pass a borrow of the actual `TurnAdmission` during preparation/quote/reservation and retain the
  owned admission in the generation lifetime. Do not add a publicly constructible certificate
  containing only `(pid, started_s)`. An exact-count capability also names its template and proves
  that changing output controls does not change the quoted prompt.
- Expose the grant owner's reservation as the existing non-cloneable capability. Introduce only
  the narrow consuming unsent/terminal settlement operations required by this transition; the
  dispatcher does not inspect or maintain a duplicate reservation map.
- A successful HTTP header is not a terminal receipt. “No token received” is not proof that a
  generation was never accepted. Release an unsent reservation and retry only for an outcome
  whose transport/backend contract proves non-acceptance (for example, connection establishment
  refused). A 503 requires that explicit backend guarantee; status alone cannot license replay.
- Dropping a future after sending can occur before headers arrive. The retained operation must
  still own its reservation and endpoint/page custody. It cannot disappear with the waiting
  caller. Reuse the existing unresolved-operation owner, with a typed completion or incarnation
  fence to settle it; no leaf cancellation cleanup that declares the resource free.
- If recovery requires a lifecycle writer, do not await that writer while retaining its read
  admission. Transfer uncertain work to custody, release only what is proven releasable, and
  re-enter admission on a fresh attempt. Reuse the operation's queue entitlement, never its quote.

Focused validator matrix for this interface:

| Scenario | Required observation |
|---|---|
| Recorded Kimi and Sahar feasible prediction errors | One exact quote, one generation, same prompt, maximal authorized reply and identical sent receipt; no cognition resize retry |
| Fixed request and insufficient adaptive minimum | Fixed cap unchanged or typed refusal; insufficient minimum sends nothing and requests genuine content refit |
| Controlled lifecycle writer during quote | Existing read admission prevents replacement until the operation releases or transfers ownership; no new lock |
| External crash / same-URL replacement / stale grant | Old executable attempt cannot POST to the new incarnation; new attempt re-admits and re-quotes |
| Cancellation before reservation / before POST / after possible acceptance | Respectively no reservation, proven-unsent release, or retained unresolved custody; never double settlement |
| Late response / duplicate terminal / connection refusal / ambiguous disconnect | Identity checked, exactly one terminal settlement, retry only with non-acceptance proof |
| Backend output controls alter tokenization | Capability refuses the one-quote optimization unless the backend can certify the final prompt identity |

Implementation order: first replace the provisional arithmetic authority and prove its properties;
then implement these private ownership transitions and adversarial transport tests; then migrate
all caller/result contracts and delete the resize branch in the same integration slice. Publish
that coherent slice only after compiler-enforced caller coverage and validators pass. Observe the
real same-prompt retry count and useful completed work afterwards. Cold-prefill, physical paging
and fairness work remain separate measured causes and use the same ownership/receipt interfaces.
### 5. Engine memory (where her KV lives)

- **Owner:** the serving daemon over ONE separate llama-server process (SERVING-ENGINE §6 wins over
  CLAUDE.md 3b's "in-process"; CLAUDE.md is corrected).
- **Invariant: physical conservation.** On unified memory, weights + every mind's resident KV +
  staging are counted ONCE against the physical pool; a grant or plan never spends bytes that are
  not released or verifiably adopted.
- **Launch:** sized from the RESERVED FIT at a `Settled` reading, and Settled means the resource
  authority OBSERVED the previous engine's release or verified adoption, never elapsed time or a
  free-memory threshold (branch `fable/launch-from-fit-pool`, on #5770's credit).
- **No relaunch to resize.** Pool growth is deleted only after launch-from-fit is proven (#5774 is
  blocked on it). Live resize is the fork's `reconfigure` verb later (SERVING-ENGINE S2).
- **Paging:** the actuator for every move of residency (rotation, eviction, restart). Pages are
  saved and restored per mind; a timeout stays `Unresolved` and HOLDS custody (#5772); CPU activity
  proves liveness, not progress.
- **Cancellation is not termination:** after a stop, the engine kept prefilling ~1.5 min (Cormac);
  a stop must cancel in-flight prefill in the fork (`should_stop` checked inside prefill), and until
  it does, the custody model assumes the engine is still working.

### 5a. The engine is a cache; prefill is the miss penalty (Joel, 2026-11-04: "It's more like a cache")

**The model is hybrid (partly recurrent):** every resume logs `pos_min = slot tokens - 1`, so a
sequence's state resumes only at its end or at a saved CHECKPOINT; middle edits (ring buffers, KV
shift, `seq_rm` of a span) do not apply to this model. A mind's state is a SNAPSHOT.

| Level | Holds | Hit cost (measured) |
|---|---|---|
| L1 | a mind's live sequence state on the GPU | 0 |
| L2 | her state snapshot in RAM | sub-second (to measure) |
| L3 | her page on disk | 0.3-3.7 s |
| miss | re-prefill | minutes (318 s for 76k tokens) |

- **Lines are checkpoints**, placed at stable boundaries (end of the shared head; end of each
  appended act), so every prompt resumes from a recent line. Residents' prompts do share their
  prefix act to act (`delib.prompt.common_prefix` 37k-190k chars, 2026-11-04 ~22:00Z); the
  `raw lcp = 5` full re-prefills in the engine log are the SCRATCH slot (id 4), where unrelated
  non-turn requests overwrite each other: expected, small (~800 tokens), and a candidate for its own
  line per purpose rather than one shared scratch.
- **Replacement** by activity temperature (goal 9): hot minds in L1, cooling write back to L2, L3.
- **Write-back at turn boundaries**, while she is in a tool call, never in the GPU's way.
- **Prefetch:** the next mind's line warms while another decodes; a tool's result prefills as it
  streams in (plan line 13).
- **Coherence:** eligibility requires the full compatibility/provenance contract in 5b, plus
  a matching immutable prefix/checkpoint revision. A compatible artifact can still be deferred
  for capacity; a successful restore can still be followed by a prompt rebuild. Neither HTTP
  status nor cache-hit count alone identifies the cause.
- **The metric includes hit rate and avoided work.** Classify compulsory, capacity and conflict
  misses with evidence; retain Unknown where the causal receipt is missing. On 2026-11-06 all four
  M5 pages restored successfully, while Atlas rebuilt its view and Aris later had a cold turn
  without a restoration event after engine relaunch. Do not generalize the earlier conflict
  diagnosis to every cold prefill or disable restoration/growth on that basis.

### 5a.1 The cache, measured on one node (M5, 2026-11-04 23:29-23:40Z)

Codex's harness (`measure.py`, warm_first, page_pair; synthetic source, exact token slices, a
different 128-token tail, 3 repeats each), Qwen3.6-35B-A3B Q4_K_M (sha 671e47e0ec53c665), engine
binary sha 0a3cb69808cbcabe, live flags on 2 slots (`-c 65536 --parallel 2 --kv-unified`, fa on,
KV q4_0, ubatch 2048, spec draft-mtp, ctx-checkpoints 32, cache-reuse 256; mmproj off). Raw:
gist joelteply/0225fe9bb4fd328d07f9b50d35628abd and `~/.continuum/evidence/codex-harness-20261104/`.

| Case | First token | Cached / computed |
|---|---|---|
| cold 4,096 | 4.56 s | 0 / 4,096 |
| warm 4,096 + 128 tail | 0.354 s | 4,092 / 132 |
| cold 12,288 | 15.25 s | 0 / 12,288 |
| warm 12,288 + 128 tail | 0.395 s | 12,284 / 132 |
| page save 12,288 (315,981,752 B) / restore / restore + first token | 0.16-0.21 s / 0.04 s / 0.44 s | 12,284 / 132 |
| cold-first mixed, WITHOUT `--no-cache-idle-slots` | warm 11.65 s | 0 / 4,224 (idle slot cleared) |
| cold-first mixed, WITH `--no-cache-idle-slots` (the live flag, #5764) | warm 7.11 s | 4,092 / 132 |
| warm-first mixed (flag absent) | warm 0.35 s, but decode 32.7 vs 78-95 tok/s | 4,092 / 132 |

Read: a hit is ~35-39x a miss on this node (L1 0.4 s, L3-warm 0.44 s, miss 15 s for 12k). The
page restore is same-process and filesystem-warm, NOT a cold-disk or restart proof (owed). The
idle-slot clear was a bench configuration gap; live engines already pass the flag (#5764 enforces
it on launch and in the reconcile). With the cache intact, a cold prefill still costs a warm mind
7.1 s of first-token delay when it arrives second, and ~60% of its decode rate when it arrives
first: the 5d bound is the next lever. 5090 (Codex, Qwen3.8-27B) shows the same shape.

### 5b. The fast path is the design, not an optimization

The fast path existed and was measured on 2026-11-04: a page SAVE 1.5-3 s, a RESTORE 0.3-3.7 s,
against a 318 s cold re-prefill (~100x). It was defeated, not missing:
- restores refused `status=400` after relaunches, because the new engine came up at a different
  pool size and a page saved under the old window does not fit the new slot (pools that day: 33k
  to 654k for the same build);
- 52 turns went cold with `inference.kv_page.cold_turn cause="page_on_disk_unconfirmed"`: a page on
  disk (31 GB in `~/.continuum/cache/kv-pages`) the ledger did not trust after a relaunch.
**Invariants:** (1) an engine's geometry is STABLE for its life and across core restarts (one launch,
sized once from the settled fit; no relaunch to resize); (2) a page is eligible for restore only on
its full fingerprint and complete-save provenance: the persisted ledger
(`slots/saved_ledger.rs`, LEDGER_FORMAT 2) already fingerprints model, adapter and draft revision, KV
type, flash attention and engine build and excludes window/slot count; confirmation is preserved,
and no file is trusted by its header alone; a larger destination is acceptable only if the fork's
restore validates layout and token capacity, never by a size comparison; (3) every miss is a loud,
classified defect (plan line 6), never a quiet five-minute prefill.
**Restart adoption (Cormac, IntelMac 22:05Z, cf2e8089f):** the boot sweep logged
`LeftLive { pid: 78294 }`, then `serving.genome.in_place_unsupported from=0 to=1`: adoption read a
running engine's adapter set as empty while it served `--lora` Delia's adapter, so the core
relaunched twice in a minute and dropped every mind's KV. Adoption must read the engine's actual
adapter set (the fork's typed report), or every core restart on an adapter-bearing node is a cold
restart.

**Diagnosis still owed (Codex), before any fix:** `page_on_disk_unconfirmed` conflates an interrupted
save with a ledger-load failure; it does not prove a relaunch forgot confirmation. For the same page
and incarnation on the deployed revision we need: the ledger-unread reason, the format/fingerprint,
the save receipt, and the restore 400's body.

### 5b.1 Accepted restore-refusal contract and first execution slice

Codex accepts this contract (2026-11-04), based on the audit supplied by Joel and Fable.
This is agreement on this slice, not blanket approval of the draft or an implementation receipt.

**Diagnosis:** the audited restore 400s report insufficient unified-cache cells (59,144
requested in a partly occupied 125,696-cell pool) or a page exceeding total capacity
(35,838 tokens versus 33,280 cells). They do not report model/format mismatch. The core
reduces the response to HTTP status and calls `note_page_lost`, removing persisted save
confirmation. Later admissions see `page_on_disk_unconfirmed` and prefill again. The reported
ledger-load counts fall from four minds at 19:38 to zero at 21:04. Aris's confirmed 19:45
save missing at 19:58 without a refusal remains a separate unexplained loss; this fix must
not claim to explain it.

**One typed outcome at the engine/core seam:**
- Restored: verified successful receipt, identifying the page and destination incarnation.
- DeferredCapacity: distinguish insufficient currently allocatable cells from a page that
  exceeds the whole compatible pool. Preserve the durable confirmed page in both cases.
  Retry transient pressure after verified reclamation; no geometry change is required.
  Oversize requires a feasible capacity/placement change, not repeated restore attempts.
- Incompatible: retain the saved artifact and its provenance; it is ineligible for this
  destination, not necessarily invalid for every compatible destination.
- InvalidPage: only positive evidence of a missing, corrupt or incomplete artifact invalidates
  that artifact's confirmation. Do not infer this from HTTP 400 alone.
- Unresolved: transport failure, unknown reason or timeout retains provenance and operation
  custody until a terminal receipt or fencing proves the outcome. Never fabricate free cells.

Preserve engine reason codes and available numeric evidence through the core; unknown fields
stay unknown. Capacity refusal must not route through `note_page_lost`. Do not delete the
artifact as a side effect of deferral or destination incompatibility.

**Fit and ownership:** persist page token count with its complete-save receipt and identity.
Admission compares that requirement with total compatible capacity and currently allocatable
cells, reserves through the existing authority, and makes room through verified custody
transfer before restoring. A free-cell snapshot alone races other admissions: the engine's
restore result remains authoritative. A save receipt alone is not evidence that resident KV
was erased; release must be verified separately when the operation does not release it.
Launch must reserve enough capacity for the admitted page working set within the physical fit;
it does not promise to fit every disk page simultaneously or exceed physical memory. An
unplaceable page gets an explicit deferred/placement outcome with its saved state preserved.

**Execution owners** (assignments sent over AIRC; peer acceptance tracked separately):
- Codex: engine typed restore outcome, core propagation, page custody and ledger token metadata;
  first execution slice after the failure reproductions exist.
- Fable: recorded audit fixtures, launch-fit and admission integration; shared design updates.
- Cormac: independent durability/restart review, including the unexplained Aris ledger loss.

**Required proof using the existing fixtures:** capacity refusal preserves confirmation across
restart; verified reclamation permits a later restore without cold prefill; oversize stays
confirmed and deferred; incompatibility preserves the artifact; proven corruption invalidates
only the affected artifact; concurrent admissions cannot double-spend free capacity; timeout
and late receipt retain custody; completed save and ledger replacement survive interruption.
Reproduce the reported loss before the fix, then verify the corrected transition through the
real engine adapter and persisted ledger. Report the unexplained loss independently until it
has its own reproduction. Follow with the live restore/cache-reuse receipt.

### 5d. A cold prefill adds only bounded latency to a warm mind's next token (Cormac's matched run, IntelMac 22:05Z)

Mixed load at 9 (4 cold 4,641-token prefills + 5 warm decodes), same binary/model/adapter as the
lane, CPU, `--ubatch-size 2048 --kv-unified`: the prefills complete strictly serialized at 102 /
173 / 243 / 319 s and ALL FIVE decoders finish at 335.6 s; once running they decode at 7.8 tok/s
each. At n=2 a decoder finishes at 88 s behind an 81 s prefill. Raw:
`~/.continuum/evidence/knee-20261104/` (IntelMac). Decode concurrency scales (19.9 -> 51.1 tok/s
aggregate, 1 -> 9); prefill has a capacity of one on this node.
**Invariant (Codex's wording):** under mixed load a warm mind's decode suffers only BOUNDED
additional latency from other minds' cold prefills (a non-preemptible kernel or chunk still takes
its time), AND every cold prefill eventually completes, so strict decode priority cannot starve a
cold mind. Mechanism in the fork's own scheduler, reusing its measured service curves: prefill
proceeds in bounded chunks sized from the measured chunk time when decoders wait; the number of
concurrent cold prefills is a MEASURED admission limit for this backend and configuration, never
universally one; cancellation takes effect at chunk boundaries; recurrent (hybrid) state stays
intact across chunks and cancellation. Validated by re-running the mixed trial: decoders'
first-token and inter-token latency within their bound, and every prefill completing.

### 5e. A continuous pipeline, shaped like bgfx (Joel: "just like a graphics pipeline. How I did bgfx")

The engine executes dependency-ready work; the core encodes intent. It can overlap stages only
when dependencies and resource reservations permit. A pipeline does not create extra prefill
throughput or justify speculative work while useful work waits.
- **Encoders record, the device executes.** Every mind's work (her appended tail, a tool result as
  it streams in, a page-in, a write-back, a view rebuild) is recorded as commands the moment it
  happens, by whoever produces it, subject to bounded submission admission.
- **Double-buffered frames.** An engine step is the frame: commands recorded during step N execute
  in N+1. Submission never blocks on execution.
- **Scheduling.** Dependency readiness and feasibility precede priority. Use the algorithm below;
  a lexical warm-decode-first sort alone cannot establish fairness or reclamation progress.
- **Handles and deferred destruction.** Sequences, pages and snapshots are handles; a release
  takes effect only after the engine's receipt for the frame that used it (the custody of 5 and
  5b.1 is the frame fence).
- **One device owner:** one dispatcher per engine (stage 4).
**Codex's conditions (agreed, 2026-11-04):** (1) submission is non-blocking only behind BOUNDED
queue admission with typed backpressure: no unlimited command or staging growth; (2) only
dependency-ready commands are sorted, per-sequence causal order is preserved, and aging/service
bounds keep decode from starving the prefill or write-back that reclamation needs; (3) a frame
fence covers actual device completion, plus the durable save receipt where a release depends on
it, never mere dispatch or HTTP acceptance; (4) streaming tool prefill uses only the COMMITTED,
append-only rendered and tokenized prefix, never mutable partial JSON or a view later compacted
under the same sequence. "N+1" is an execution epoch, not an artificial wait for a frame. A
shadow sequence reserves its destination and staging before it is allocated. Completion has distinct receipt DOMAINS under one scheduling authority (CPU, GPU copy, GPU
compute, durable I/O): a disk save completes in its own domain and is never forced synchronously
onto the device thread; a release that needs it waits on that domain's receipt. The paging outcome
slice (5b.1) remains first.
What it buys, each already named: prefill of a tool result while the tool runs (plan line 13);
page-in ahead of the turn (`inference.kv_warm_ahead`, which logged `skipped` on 2026-11-04 and
needs its reason classified); write-back of cooling minds during decode; the next collapsed view
built in a shadow sequence and swapped in at a turnover (a fresh sequence, valid for a hybrid
model); CPU cores rendering, tokenizing and embedding in parallel with the GPU.
**Gate:** GPU idle fraction while any mind waits on work the GPU could already be doing, measured
before and after; and per-act wall time split into tool, wait, prefill and decode.

#### Interfaces and ownership inside the existing pipeline

**bgfx reference, inspected 2026-11-06.** Its [internals](https://bkaradzic.github.io/bgfx/internals.html)
separate encoder recording from render-thread execution. Submit/render frames exchange ownership;
frame advancement synchronizes, so double buffering does not mean unlimited nonblocking submission.
View modes explicitly distinguish reorderable work from sequential work. Resource commands are
deferred behind handles rather than executed by each caller.
The [source](https://github.com/bkaradzic/bgfx/blob/master/src/bgfx_p.h) makes the boundaries concrete:
`EncoderImpl`, `Frame`, `m_cmdPre`/`m_cmdPost`, per-frame free-handle collections and transient storage.

**Application here (our design inference):** keep encoder state private; seal immutable command
batches; distinguish reorderable independent work from per-sequence causal work; and make lifetime
transitions the execution owner's responsibility. Retain backend-specific completion domains.
Unlike graphics frames, a mind's operation can span minutes and durable I/O: do not impose a global
frame barrier that waits for every mind or confuse submission-buffer reuse with GPU/disk completion.
The transferable abstraction is ownership transfer through a narrow interface, not the literal
frame cadence or draw-call sort key. Existing long-lived owners provide it; leaves only encode intent.

The names below describe contracts to implement through the existing owners, not new daemons
or a parallel framework. Encapsulation means a caller cannot reconstruct policy from public
counters. Composition connects owners; backend implementations supply capabilities and execution,
not their own queues, fairness rules, cancellation loops or accounting.

| Owner | Input / output | Sole responsibility | Leaves must stop doing |
|---|---|---|---|
| Context codec / encoder | Activity state + committed results -> immutable prompt revision and required working set | Rendering, disclosure, stable prefix/checkpoint identity | Ad-hoc truncation, separate token budgets, mutable streaming-prefix guesses |
| Existing engine dispatcher | Typed work intent -> operation handle, deferred reason or terminal result | Dependency graph, bounded queues, fairness, exact dispatch decision | Readiness polling, local queue retries, final-cap calculation |
| Existing grant/page authorities behind the dispatcher | Attempt identity + resource claim -> reservation / typed refusal / release receipt | Physical conservation and verified custody transitions | Treating free snapshots, cancel acknowledgement or save acknowledgement as released memory |
| Existing serving daemon | Lifecycle intent + observed engine identity -> verified engine capabilities/incarnation | Launch, adoption, reconciliation and fencing | Spawning/reconfiguring engines from generation or tool handlers |
| Backend executor | Executable command + reserved handles -> domain-specific completion | Token counting, prefill/decode/copy/save mechanics and progress | Inventing caller policy or weakening admission |
| Outcome consumer | Sent/terminal receipt -> captured result, allocation and learning observation | Interpret the authoritative result | Recalculate what was sent from render-time estimates |

Submission returns a durable-in-owner operation identity and a completion subscription. Queue
admission accounts command metadata and staging bytes as well as device resources. If full, return
typed backpressure with an owner-driven readiness notification; producers do not busy-poll or
allocate an unbounded deferred list. Existing daemon wakeups consume receipts and dependency changes.
Persistence is required for state/custody that must survive restart, not for every transient frame.

#### State machine and scheduling algorithm

One operation moves through `Submitted -> Waiting(dependencies/resources) -> Reserved -> Running
-> Settling -> Terminal`. `Deferred` and `Refused` identify whether the same operation can be
awakened by a named event or requires changed caller input. Cancellation before execution retires
the queued operation and releases its reservations. After execution begins it requests cancellation
and remains in Settling until matching terminal completion or verified incarnation fencing. Duplicate
receipts are idempotent by operation and attempt identity; stale receipts cannot settle another attempt.

At a submission, completion, verified release, or relevant lifecycle event, the dispatcher:

1. Applies the identified event once, updates custody, and wakes newly dependency-ready commands.
   Per-sequence commands retain causal order; independent CPU/I/O work can proceed concurrently.
2. Forms the feasible set under conserved residency, staging, and backend execution capacity.
   A blocked command names its dependency or shortfall. Capacity-releasing prerequisite work
   inherits the urgency of the admitted work it blocks; background write-back cannot be starved
   while all demand waits for its release.
3. Selects among feasible commands using a work-conserving fair service policy. Track per-mind
   virtual service in measured execution time, charge actual completed work, and retain debt across
   requests; request splitting must not buy priority. Activity temperature selects documented
   service weights. An explicit aging/service guarantee overrides opportunistic warm preference.
   Calibration and weights remain an open measured policy parameter, not invented constants here.
4. At a backend-safe chunk boundary, services due decode and aged prefill/reclamation work. Choose
   prefill chunk size from the measured service curve and allowed additional decode delay; if the
   smallest legal chunk exceeds that delay, report the physical limitation. No strict decode-first
   rule that can starve prefill, and no universal cold-prefill concurrency of one.
5. Reserves through the existing authorities, emits executable commands, and waits asynchronously
   on their domain receipts. No device-thread wait on durable I/O. Recomputes feasibility on the
   next event; it does not infer freed capacity from an elapsed estimate.

The pure scheduling policy is separate from asynchronous actuators but is used by the real dispatcher
and validator. Before selecting a concrete fair-queue implementation, replay both arrival orders,
heterogeneous chunk durations, dependencies and saturated demand against the service guarantees.
Do not implement a sort-key heuristic and label it a proven bound.

#### Exact dispatch is a refinement of an admitted operation

The exact decision specified in section 4a belongs inside `Reserved`, after endpoint/page custody,
before generation. A feasible output-room correction keeps the same operation and queue entitlement;
it does not create another cognition retry. The executable value alone builds wire fields and the
sent receipt. Engine/grant invalidation requires a fresh attempt and quote under the same owner.
The context encoder only re-renders when its required working set actually cannot fit.

#### What overlap is permitted

CPU rendering may overlap another mind's decode; verified page write-back may overlap compute in
its own completion domain; committed append-only tool output may prefill behind a stable prefix.
Each overlap reserves its staging and destination first. Mutable tool arguments, incompatible
checkpoints and view rebuilds cannot be appended to a live recurrent sequence. A rebuilt view uses
an admitted shadow sequence and an atomic ownership switch at its dependency fence. Measure whether
restoring/promoting context saves useful work enough to pay its rebuild; do not assume either that
all restoration is beneficial or that all cold prefill is a restoration defect.

#### Learned optimization uses the same policy boundary

Joel's 2026-11-06 direction extends the observe/reproduce/correct/validate/verify cycle to
networks: train an initial optimizer in deterministic simulation, then refine it with real
execution evidence. This is a future implementation option behind the existing scheduling and
placement policy interface, not another owner, scheduler or training subsystem.

A learned policy may propose service order, placement and cache retention from observed state.
The deterministic resource owners validate feasibility, conservation, custody, cancellation and
legal transitions; learned scores cannot relax those invariants or manufacture terminal proof.
Record observation, candidate action, admitted action, policy identity and measured outcome so
training does not confuse a rejected proposal with executed work. Reward must account for useful
completed work, latency, fairness and resource cost, not raw activity or token volume. Evaluate on
held-out recorded scenarios and adversarial interleavings, then compare shadow decisions against
real outcomes before adopting through the existing stable/shadow learning lifecycle. Simulation
is an initial environment model; live discrepancies become new reproductions and training cases.
No learned optimizer is required to complete the current deterministic dispatch correction.

#### Incoming-agent work package: learned optimizer

This is an independent side workstream requested by Joel, ready for assignment when collaborators
return. Recover existing learning and scheduler work before claiming it. Its first deliverable is
a reproducible simulator experiment and an integration proposal against the existing policy seam;
it must not delay or independently modify the dispatch/custody correction above.

**Scope the first experiment:** choose one decision domain (for example, selection among feasible
ready operations), expose the current deterministic policy as the baseline, and train a candidate
on recorded workloads plus seeded variations. Do not combine scheduling, placement and cache
retention into one unexplained reward initially. Preserve a versioned observation/action schema:
operation identity and age, feasible actions, owned-resource observations, measured service costs,
policy/version, proposed action, admitted action, and terminal useful-work outcome. Unknown costs
remain explicit; the network must not receive future completion information as an observation.

**Deliverables for review:**

1. A scenario corpus with provenance, deterministic replay seeds and separate training/held-out
   workloads, including cold/warm prefixes, all-pinned waits, heterogeneous service times and
   resource pressure. Use the production owners and decision interfaces; do not train against a
   duplicate implementation of their invariants.
2. A baseline-versus-candidate report covering useful work per unit time, latency distribution,
   starvation/service debt, memory and movement cost, and policy inference overhead. Document
   reward terms, normalization and tradeoffs; reject improvements bought by lost work, unfairness
   or relaxed custody. Compare with the deterministic baseline, including under workload shift.
3. A versioned candidate artifact and reproducible training/evaluation recipe using the existing
   learning infrastructure. Include an ablation showing which observations/actions produced the
   improvement and the failure cases where it loses.
4. A shadow-mode adapter at the existing policy interface. It records proposals without owning
   resources or executing side effects. Define the subsequent bounded real evaluation and
   stable/shadow adoption criteria before activating a learned policy; retain deterministic
   resource validation for every proposed action.

**Integration boundary:** the optimizer owns scoring/proposals and training evidence. The engine
owners retain feasibility, reservation, lifetime, cancellation and terminal settlement. Coordinate
shared schema changes with those owners; no second queue, daemon, priority system or leaf-specific
policy. Completion means held-out simulation results plus real shadow evidence and a reviewed
adoption plan. It does not mean merely that a network trained or its training reward increased.
#### Work boundaries for incoming collaborators

| Workstream | Interface to agree first | Deliverable / deletion requirement |
|---|---|---|
| Context encoder | Immutable revision, required claim, committed append boundary | One renderer/allocation path; remove leaf sizing and duplicate final-cap decisions |
| Dispatch/admission | Operation/attempt identity, output intent, executable command, sent receipt | One exact admission algorithm; remove resize-only requeue and competing guard policy |
| Page/lifecycle | Reservation and typed domain completion/fencing | Conservation/restart tests; remove inferred release and independent cancellation custody |
| Backend scheduling | Capability and measured chunk/service observations | Shared policy validator plus executor integration; remove independent priority/retry rules |
| Independent validation | Same production policies, recorded inputs and adversarial event schedules | Reproductions, interleaving/property tests and live end-to-end measurements |

These are proposed boundaries, not claims on other agents' active cards. Recover current work and
agree the shared types before parallel edits. Integrate one complete vertical contract at a time;
each slice deletes its superseded paths and leaves the architecture runnable. The unpushed
quoted-room prototypes are evidence for design review, not a mandate to preserve their API.

#### An act is ready before it is given the go (Joel, 2026-11-06)

"One act is ready before it's given the go. So you've got many cores and processes that are not
waiting on one another." Her slot is kept current continuously. Every stimulus, fact, tool chunk
and her own reply is appended and prefilled as it arrives, in dependency order, sort-keyed like any
other frame work. When she decides to act, the only device work left is the delta that arrived
after the last appended item, plus decode. Each mind's host encoders (render, tool execution, page
I/O) run on their own cores, so no mind's host work waits behind another's. The device queue is the
only shared stage.

The gate is the share of an act's fresh tokens prefilled before its go, together with act latency
split into queue, final-delta prefill and decode. The dyno's pipeline scenario (#5801, "e-ready")
scores this against today's recorded hour before the dispatcher is built.

#### Stable prompts across minds: a central provider keeps the books (Joel, 2026-11-06)

"You need a system that can maintain more stable prompts as it streams even between persona. This
is a matter of bookkeeping as part of the consumers and centralized provider."

Per-mind stability (#5802, #5803) is not enough. The rendered content must have one provider, and
every mind is a consumer of it.
- **Provider (the context ledger):** owns canonical segments, each rendered once, content-addressed,
  in byte-identical form for every mind that includes it. Segments include the system head, each
  tool set, the room wall, the board, each tool result and each fact. A streamed event appends to
  its segment once.
- **Consumer (her view):** her sequence is an ordered, append-only list of segment references. The
  ledger's order rules decide placement, never per-act rendering. She records what she sent, and the
  next act appends references.
- **Engine bookkeeping:** which segments are resident on which slot, and to which token.
  - A sequence that needs a segment another slot already holds gets it by sequence copy (`seq_cp`)
    from the holder, not by prefill. This is section 3b's promise ("shared prefixes are copied,
    never re-prefilled"), and the books make it possible.
  - Placement prefers the slot holding her longest resident prefix. Atlas's 1,818 s cold act ran
    on a slot holding another mind's prompt.
- **Gate:** tokens prefilled per act that some slot already held (shared-prefix waste), and the
  segment-reference share of each mind's prompt.

**Measured, 2026-11-06 (M5, ed142f14c, 16:13–19:17Z, 162 prompts, engine-counted): the prize is
per mind, not across minds.** Fresh tokens split as follows:

| Class | Share | Seconds |
|---|---:|---:|
| Already in another mind's prefix (copyable) | 1.9% | 651 |
| Carried by another mind, at a different position | 4.6% | 1,372 |
| She had already sent it herself (results, thoughts, wall, awareness) | 53.2% | 11,808 |
| Genuinely new | 40.3% | 12,767 |

- Token hit rate: 85.1% today, 87.8% with perfect per-mind custody and an append-only sequence,
  88.0% with cross-mind trunks added, and a 92.7% bound.
- Reordering by sharing or stability every act drops the hit rate to 54%, because it breaks each
  mind's own append order. So placement changes only at an epoch start and never reshuffles an
  epoch in progress.
- The cross-mind work is limited to two cheap changes: put the tools all minds share first, in one
  fixed order, and place the identity line after the shared head. Together they give a 4-mind trunk
  of 6.8–10.7k tokens.
- The engine copy (`seq_cp`) and residency books stay worthwhile for per-mind custody and placement
  (Benchy and Atlas's cold acts), not for cross-mind sharing.
- Evidence: Fable's scratchpad `xshare/`.

**It does not have to be sequential (Joel).** Across minds the engine's sequences form a tree, not a
line.
- Shared segments sit near the root and are prefilled once. Each mind is a branch, forked by
  sequence copy where her content diverges, as in a radix-tree prefix cache.
- Work on different branches runs in parallel and out of order, whenever its inputs exist.
- The ledger keeps the tree's books: segment, branch point, and holding slot.
- **The model's limit:** this hybrid model's recurrent layers make the state order-dependent along a
  branch (the engine reports its memory cannot shift). So blocks cannot be stitched in arbitrary
  order the way position-independent caching does on attention-only models.
- **So the ledger orders segments by how widely they are shared and how stable they are.** Most
  shared and most stable go nearest the root, so the trunk is long and branches split late.

**Engine boundary audit (Codex, 2026-11-06; source, not live proof).** At fork
`0d7dda6ae`, `server-context.cpp::get_available_slot` restricts its LCP search to
`task.id_slot` when the core specifies a slot. Core `KvSlotPool::lease` prefers the
same `PageKey` holder, then a free index; it has no canonical token-prefix directory.
Consequently a core-chosen slot cannot rely on the fork to search other holders.
`copy_state_to` copies the complete sequence only at `SLOT_STATE_DONE_PROMPT`, including
prompt bookkeeping, and is used for child tasks. The HTTP slot API exposes save,
save-size, restore and erase, not cross-slot sequence copy. Existing `seq_cp` is not
an executable cross-mind cache-hit contract.

The first engine interface must therefore carry an **exact prefix path**, model/template/
adapter fingerprint, engine incarnation, sequence revision and recoverable token boundary.
A content segment hash alone proves neither token identity nor recurrent-state compatibility.
An engine-owned operation must validate and retain both source and destination custody,
preserve a displaced destination through the existing page custodian, then copy state and
publish the destination revision together. Acquire physical slots in a consistent order;
never retain one while waiting indefinitely for the other. Invalidated source, engine change,
unsupported recurrent boundary and unavailable custody return typed outcomes without a copy
receipt. General allocation continues through the existing owner; no second slot allocator.
The initial actual-owner reproductions are: an explicit wrong-slot assignment cannot mask a
longer compatible prefix elsewhere; a source mutation between lookup and execution invalidates
the hit; a failed copy cannot publish destination residency. Only confirmed engine receipts
advance the directory. Coordinate this with Fable's stale-restore repair and canonical ledger
interface before overlapping edits. This specifies the next slice; it is not implemented yet.

**It is a cache (Joel: "It's more about reuse like a cache, no?").** The tree is just the shape of the
cache's contents when lines are shared.
- Each segment's KV state, reached by an exact prefix path, is a cache line. Its key is the hash of
  the path, not the segment alone, because state is order-dependent on this model.
- The ledger is the tag directory. The hierarchy is section 5a's: L1 live slot, L2 RAM snapshot,
  L3 disk page.
- **Hit:** any level holds the path, so it is reached by sequence copy or restore. **Miss:** prefill.
- **Policy:** maximise token hit rate by putting stable, widely shared segments first so paths
  repeat; evict by value (sharing count × rebuild cost), never recency alone; prefetch the lines a
  mind will need next.
- **The one metric is token hit rate,** with prefill seconds as the miss penalty, per mind and
  across minds.

**Automatic, never static (Joel: "make it automatic and not static so it can be efficient even with
dynamic data").** Placement, collapse depth, eviction value and prefetch horizon come from
measurements the ledger keeps continuously, never from a hand-written class list. Per source:
- change rate between acts, decayed so it tracks recent behaviour;
- sharing count across minds;
- miss cost, the tokens after its position.

The order is a value function of these, recomputed at each epoch start with hysteresis so it does
not flap. A source that turns volatile drifts tailward on its own, and one that settles drifts back.
A new source is typed `Unmeasured` and starts at the tail until measured.

Turnovers make the collapse append-only: an item collapses once, content-addressed, and successive
epochs share the collapsed log as a prefix. The collapse depth is chosen per mind from measured
growth and turnover cost. The next epoch is prefilled on a shadow sequence in idle device time once
measured growth predicts the overflow. A learned policy may later replace the value function behind
the same boundary.

**A continuous compression algorithm (Joel).** The context codec (MIND-CONTEXT) is a streaming
compressor across minds:

| Compressor | Our system |
|---|---|
| Keyframe | Epoch start or turnover: rare, and mostly references to the last keyframe |
| P-frames | Appended deltas |
| Dictionary and back-references | The ledger and cache hits; a prefix copy between slots is a long back-reference |
| Adaptive model | Per-source change-rate and sharing measurements |
| Rate control | The one allocation choosing each item's level |
| Bitrate | Prefill tokens per act |
| Compression ratio | Token hit rate |

Target: per act, the bitrate is only the genuinely new information.

**Bet on what is coming (Joel: "your algorithms can bet on the ones they expect").** Beyond keeping
what recurred, the policy predicts and prefetches, the way a CPU uses branch prediction and
speculative execution. It bets on:
- which mind acts next, and pages her in before admission;
- which segments her next prompt will hold;
- when her next turnover lands (the shadow epoch);
- the stable part of her next act's prompt.

Bets run only in idle device time, at the lowest sort key, preemptible at the next frame. A wrong bet
therefore costs only idle time and never delays ready work. The scorecard adds prediction accuracy,
the useful-prefetch rate, and device-seconds spent on bets that did not pay. The predictor starts
from measured frequencies and recency; the learned optimizer may replace it behind the same policy
boundary.

### 5f. Extend the pipeline to the grid: place work near warm state

Joel requested this direction be recorded on 2026-11-04. This is the extension of stages
4, 5b.1, 5d and 5e, not a second scheduler or a claim of measured grid speedup.
**Sequence:** prove local custody, typed restore outcomes and bounded mixed-load scheduling
first; then carry the same command, reservation and completion contracts across nodes.

**Owner:** Codex owns the dispatcher and placement contract; Fable owns the core encoders.
The existing grid routes typed work to the selected engine. Each engine remains the one
owner of its device; placement requests admission rather than bypassing that owner.

**Placement compares complete paths, not idle GPUs.** For the same compatible work, compare:
1. wait for an engine already holding the correct warm sequence, then append/decode;
2. reserve a destination, transfer a compatible saved page, restore, then append/decode;
3. send the required prompt to another compatible engine and prefill it there.
Use measured queue delay, remaining prefill/decode work, page and staging bytes, network
bandwidth/latency, restore cost and source disruption. Account for overlap only when the
actual dependency graph permits it. A local warm wait may beat a remote cold engine.
Unmeasured costs stay explicit; an idle device is not evidence that migration wins.
Avoid placement oscillation by charging the whole transition and its uncertainty, never
by chasing an instantaneous throughput sample. Preserve bounded service for every mind.

**Compatibility is a prerequisite.** A page or shared prefix is eligible only under its
model/adapter revisions, tokenizer/template identity where relevant, state format and KV
representation, checkpoint/prefix identity, and the destination's supported restore contract.
Different hardware does not imply portable KV; no transfer or conversion is assumed valid
without a verified capability. Incompatibility preserves the source artifact. Recompute on
an admitted destination remains an explicit alternative, not a mislabeled successful restore.

**One handoff, verified custody.** Reserve destination residency, transfer staging and network
work before starting. Identify the operation, immutable page/checkpoint revision and both
engine incarnations. Transfer and verify the complete artifact; restore it and obtain the
matching terminal receipt. Commit execution ownership once, fencing stale or duplicate
commands, before releasing source residency. A send acknowledgement, timeout, disconnect or
expired lease is not proof of destination completion or source release. Keep custody through
uncertainty; reconciliation uses the operation identity and verified fencing. Release any
reservation only after its work is terminal or fenced. Preserve durable saved-page provenance
independently of whether either engine currently holds the sequence.

**Overlap without waste.** Encoders may prefetch an eligible page and prefill a committed,
append-only tool-result prefix while CPU/tool work continues. Those operations use the same
bounded queues, reservations, dependency ordering, cancellation and aging as ordinary work.
Speculative placement cannot crowd out useful admitted work. Shared immutable prefixes are
reused only where the backend proves compatible sharing; a shadow view reserves its own
state before the turnover fence. CPU, network, GPU-copy, GPU-compute and durable-I/O receipts
remain distinct domains under the one scheduling contract.

**Evidence and gate.** The within-node results in 5a.1 and the
[5090 measurements](https://gist.github.com/joelteply/b2898e781c2231ca80f253e6d877812f)
justify reducing avoidable prefill and protecting warm decode. They do not measure grid
transfer or cold-disk performance. Validate grid placement with the same model and work:
local warm wait versus remote page handoff versus remote cold prefill; record end-to-end
first output, p95 decode gaps, useful completions per mind, bytes transferred, peak reserved
memory and prediction error. Exercise duplicate delivery, cancellation, stale incarnation,
network loss and restart: no double execution, no capacity double-spend, no premature source
release and no loss of a valid page. A faster transfer alone is not a faster completed act.

**Chunk evidence (M5, 2026-11-05 00:05Z; PAIRED `--batch-size` = `--ubatch-size`, not isolation of
the physical chunk; Codex's warm_first trial: a primed 4,096 + 128-tail warm request, then a cold
12,288-token prefill on the other slot 500 ms later; live flags incl. `--no-cache-idle-slots`;
3 repeats each, within ms; cache 4,092/132 kept in all; raw
`~/.continuum/evidence/codex-harness-20261104/chunk/`):**

| paired batch | warm decode | warm WORST content gap | warm p95 gap | cold TTFT | cold wall | common wall |
|---|---|---|---|---|---|---|
| 512 | 41.8 tok/s | 0.75 s | 0.047 s | 20.1 s | 21.5 s | ~22.0 s |
| 1024 | 44.9 | 1.21 | 0.047 | 17.6 | 19.0 | ~19.5 |
| 2048 (live) | 32.7 | 2.23 | 0.047 | 16.9 | 18.4 | ~18.9 |

The worst gap tracks the chunk (each prefill chunk is a decoder pause) and is invisible at p95.
This supports measured-time chunking plus service-frequency control in the dispatcher, NOT a
blanket setting: three points on one node and one workload. The eventual 5d gate keeps BOTH
arrival orders (warm-first and cold-first) and the worst-gap metric.

### 5c. The operating point is measured, never a number

No fixed count of residents, lanes, slots or sequences. RESIDENCY (which minds hold state on this
engine) and RUNNABLE CONCURRENCY (which minds run now) are separate decisions. Both are outputs of a
controller over measured throughput, including the cost of every transition (page out/in, prefill): acts per hour per mind and in total, prefill seconds per
act, cache hit rate, wait between acts. Admission (stage 4) raises concurrency while total useful
acts per hour rises and every mind's wait stays bounded, and lowers it when added minds only add
prefill contention (on the IntelMac CPU, prefill measured 68.3 / 68.0 / 63.2 tok/s at 1 / 2 / 4 streams; that curve is this hardware and workload, not a universal law). Minds are never migrated to improve an instantaneous throughput sample. The population a node hosts
follows the same measurement (personas create minds; placement is by capacity). The engine's
`--parallel` is the CEILING the launch reserves residency for, chosen from the fit, not a lane
count admission obeys. Every downward adaptation schedules its upward test.

### 6. The tool call and its result

- **Owner:** the command's handler (execution), its precondition (stage 1), and ONE renderer
  (`tool_executor/answer.rs`).
- **Invariant:** every result says what happened and her next moves; a refusal carries a typed
  reason and the moves valid now (Cormac, card 9bba067f); an identical resubmission returns the
  existing artifact (`AlreadySubmitted`), a changed one submits; ids she reads and types are `Shown`
  (card 484b000b). The result enters her view through stage 2, never sized by the command.
- **Repeats:** ONE owner: the view's repeat fact (`continuum-context/view/repeat.rs`), fed by the
  settle evidence; the other two detectors feed it or are deleted (card e03c2aa0).

### 6b. The tool contract: a call is a transaction, never cut, never clipped

Measured on the M5, 2026-11-05 02:20-02:54Z (build 0b2854327, Benchy's prompt captures). One write
landed out of five attempts:
- Two `edit_file` calls were cut inside their arguments by the output limit (JSON ended at col 126
  and col 4,033).
- Two line-range edits (`end_line 603`) ended inside a construct and were refused.
- One whole-file write of 35,796 bytes landed, to add one test.

Earlier, on a95afe3d1, 302 of 414 acts were cut mid-thought because output room was priced from the
window's leftovers. None of this is a defect in one tool. Each failure is a pattern every tool owes,
and none of them is owned in one place. Claude Code's own tools keep these guarantees, which is why
their calls do not fail this way.

One `ToolContract` every tool implements. The compiler requires it, and the ratchets forbid each
way around it:

1. **A call is never cut.**
   - Each tool's schema declares its payload fields (a write's content, an edit's new text).
   - Stage 2's one allocation reserves her measured think plus the measured payload of the call she
     is making (per tool, from her history), so the budget fits the call.
   - A `Length` finish inside a call continues the same generation from its warm prefix; it is not
     discarded and re-thought. Not yet available in the fork: see the refinement below (research, Codex).
   - A call that still cannot fit is a typed `WindowTooSmall`, never a truncated call (stage 2, #5787).
2. **An edit anchors on content.**
   - Line-range edit modes are deleted. A text edit requires a unique anchor; a whole-file write is
     for a new file.
   - Each edit carries the content hash of her last read of that file. An anchor that is stale or
     appears more than once is refused, typed, with the current region.
   - The payload is only the change, never a re-sent file.
3. **A result is never clipped.** Every result is a `continuum-context` item with levels and a handle
   (stage 2's one codec, #5779 and #5782), so no tool cuts its own output.
4. **A refusal is a typed variant with her next move** (stage 6), never prose. Today the parse guard
   already words the right next move ("anchor on the text with search_replace") in plain text.
5. **Arguments are validated against the schema before execution**, typed per field, so a malformed
   or partial call never reaches a tool.

**The next tool gets all of it for free** (Joel, 2026-11-05: "So the next new one you write gets it for
free. Never allow breaks like this."). This is the substrate's for-free triplet (CBAR) applied to tools:
- a base trait;
- a `#[derive(Tool)]` on the tool's argument struct, which generates the schema, the payload
  declaration (`#[payload]` on content fields), validation, and the typed refusal enum with next moves;
- a scaffold generator.

A tool author writes the argument struct and the body, nothing else. Continuation, payload-sized room,
content anchoring (through a shared `Anchor` argument type), and results as codec items all come from
the contract. The ratchet `tools_outside_the_contract` = 0 means a tool defined any other way does not
build.

Refined with Cormac (2026-11-05):
- **One refusal shape.** It is already proven twice, in #5782's `Unmet` and in airc's `ShownIdError`.
  The derive generates `<Tool>Refusal` with `next_moves() -> Vec<Move>`, where `Move::command()` is
  pinned to `ActionCommand::NAME`. When there is no move, the refusal carries a typed reason instead
  (`NoMove::{Held(by), Ended(state)}`). The ratchet fails a refusal that carries neither.
- **The read hash advances with her own edits.** Every landed edit returns the file's new hash, and
  her working set records it. So her second edit in one act is checked against her first edit's
  result, never refused as stale against her own write. A peer's concurrent edit is stale, and is
  refused with the current region. One test pins both.
- **Continuation is the guarantee; payload sizing is the optimization.** A tool's first call is
  `Unmeasured` and can exceed any reserve, so continuation lands first.
  - In the fork, a raw `/completion` takes the rendered prompt plus the partial output, so a warm
    prefix costs about zero prefill.
  - The tool-call parser and grammar state resume mid-JSON instead of restarting.
- **Continuation is research, not yet a guarantee** (Codex, fork source at 0d7dda6a). Today:
  - `continue_final_message` exists (server-common.cpp:1356), but an assistant message containing
    `tool_calls` is rejected (:1370).
  - `init_sampler` resets the sampler and accepts prompt tokens with `grammar=false`
    (server-context.cpp:394).

  So a raw `/completion` with the prompt plus the partial output restores the warm KV, but not the
  grammar, parser or RNG state. The seam the fork has to provide:
  - the retained sequence, plus the exact generated token suffix, plus the parser, grammar and
    sampler state;
  - continuation bounded within the leased capacity;
  - one logical call identity;
  - execution only after the parse is complete and the schema validates, with receipts and effects
    deduplicated.

  Finite context can still end in a typed `WindowTooSmall`, so there is no absolute never-cut
  guarantee. First tests cut inside an escaped string, inside UTF-8, in nested JSON, at a tool
  delimiter, and at a boundary between multiple calls; each must prove exactly one complete effect,
  and that the prefix is reused. Codex owns the fork investigation.
- **Order:** (5) schema validation and (4) typed refusals need no fork work and land first, then (2)
  anchors, then (1) continuation once the fork is verified.
- **Second gate number:** bytes re-sent per landed edit (the 35,796-byte write landed and still broke
  "payload = the change only").

Ratchets: `line_number_edit_modes` = 0; `tools_without_declared_payload` = 0;
`untyped_tool_errors` falls; `truncation_outside_the_view` = 0.

Gate, live: the share of a mind's tool calls that land, from her prompt captures (cut calls, refused
edits and landed calls per mind), together with bytes written per landed change against the change's
own size.

### 6c. One protocol, rendered from types: no prose reaches her (Joel, 2026-11-06)

"These inconsistencies should be impossible. Stop just writing text into tools and prompts. The
system ought to be more formulaic... We need a system not a bunch of prose." Also: foolproof, done
at the schema level, little burden on producers, concerns separated, and two-way ("results,
errors, causality... chained and mapped without an ability to go haywire").

**Evidence** (inventory on canary a745106a3, `~/.continuum/evidence/prose-inventory-20261106/`):
- **The root defect:** her tools go out under each command's first alias (`read_file`, `help`,
  `read_output`), but nearly all prose names the slash form. So the verbs in her text almost never
  match her function list.
- **Non-native verbs named in prose:** ten, including `tool/output` at about 11 sites, and one
  command that is an erroring stub.
- **One concept, many protocols:** four ways to end a turn, three review-verdict protocols, and
  reopening a saved result as 7 verbs in 5 call forms, whose pointer and its recogniser disagree.
- **Wrong or stale facts in prose:** wrong parameter names, constants restated, and hand-kept name
  lists naming commands that don't exist.
- **Volume:** about 125k chars of tool descriptions and about 570 error sites, with no rendering
  layer at all.

**Design: producers emit typed values, and one renderer owns presentation.**
1. **Verb references are types.** A `VerbRef` is constructible only from a native `ActionCommand`.
   It renders through the one wire-name function, so she always sees the name she can call. A
   non-native verb cannot be named, or is named with its typed discovery path.
2. **Affordances are typed values, rendered once.** "Reopen this saved result" is one variant (one
   verb, one call form, its parameter names taken from the schema), and the same holds for every
   handle, pointer and fold marker. The pointer and its recogniser are the same type, so there is no
   string parser.
3. **Each protocol exists once.** Ending a turn is `yield_turn`; the PASS forms are deleted. A review
   verdict is `work/review`. An instruction block that describes a protocol is rendered from the
   protocol's type.
4. **Outcomes are typed and causal (the two-way loop).**
   - A call parses into its verb's typed parameters (6b).
   - Every outcome (result, refusal, error) is typed, correlated to the exact call that caused it,
     and carries its cause chain: errors wrap typed sources instead of strings.
   - Every refusal carries typed next moves that are themselves valid `VerbRef` calls (Cormac's
     `<Tool>Refusal`).
   - Call → outcome → next move → next call forms a causal graph keyed by typed ids. Everything she
     perceives about her history is rendered from that graph.
5. **Descriptions come from the declaration.** A tool's text comes from structured fields (purpose,
   and parameters from its schema with real names, defaults and enums). Constants are interpolated
   from their measured or declared values, and cross-references are `VerbRef`s. No maintainer
   rationale ships.
6. **Headers and facts come from registries.** Block labels come from source ids; nudges are
   `PerceptionFact` values with interpolated measurements, in one voice.
7. **The renderer is separate from producers.** It owns vocabulary, voice and the level the
   allocation chose. A producer adds one enum variant with typed fields; the derive (6b's
   `#[derive(Tool)]` family) generates the rest.

**A formula, so a rename or a move can't break anything (Joel: "That's why a formula is necessary
all the time. Prevents errors. What if you relocate the tool and its uri or renamed it?").**
- **A reference is a type, never a name.** `VerbRef` points at the command's type. Rename the
  command and every reference renders the new name; move it and a stale path fails to compile.
- **Persisted text stores identity, not names.** Act logs, handles in checkpoints and collapsed
  items store a stable typed identity and render the current name when read.
- **A rename keeps the old name as a declared alias,** so her trained calls still parse to the same
  typed command.
- **Wire names and URIs are derived from the declaration at render time,** never written anywhere.
- **Tests:**
  - renaming a command changes every rendered reference and still parses the old name;
  - a stored act from before a rename renders the new name.

**Enforcement:**
- **Ratchets:** prose literals reaching her context (outside the renderer) fall to zero; verb
  literals (slash names in strings) = 0; hand-kept name lists = 0.
- **Compile-fail tests:** a marker naming a non-native verb, and a parameter name absent from its
  schema.
- **Gate:** an audit replay over recorded prompts. Every verb named is in her function list, every
  call form is valid, and each protocol has exactly one form.

**Order, by the inventory's worst offenders:**
1. `VerbRef` plus wire names, with the `tool/output` reopen affordance (in flight).
2. One silence protocol.
3. One review protocol.
4. `TOOLS_INTRO` and the system blocks rendered from `native_offers()`.
5. The facts registry.
6. Headers.
7. Descriptions via the derive.
8. `CommandError` typed with cause chains (Cormac's refusal shape).

### 7. Around the act

- **Learning** is a low-duty dream stage between decode steps under the governor, with its slowdown
  measured and enforced (ONE-RESIDENT-MODEL). Unbounded interference from training is
  forbidden on a serving node (IntelMac 2026-11-04, job 2f1bbe9d: resident prefill 8-13.6 -> 3.2-5.2 tok/s, about 2.5x slower, for 2 h with 0.0% reported progress). The later 9e0106d6 job completed three epochs and saved its adapter while retaining custody; elapsed job lifetime or a stale progress field alone is not proof of a deadlock. Learning shares the existing governor, with measured service interference and terminal ownership, not a separate permissive scheduler.
- **Persistence** on change, never per tick: both the volatile checkpoint (JSON, written every tick) and the per-resident airc store writes (`airc/events.sqlite` sqlx pools, which with rayon spin carry the IntelMac core's 134% CPU; card 9cf6b487).
- **Deploy custody:** a deferral is not a failure (card 90ce8f6a).

## Proof: the dyno, before anything goes live

### Core architectural method

**Observe the real system → capture the failure → reproduce it deterministically →
correct the owning algorithm → validate in simulation → verify on the real system.**

This cycle is required architectural work, not an optional testing phase. Preserve
revision, inputs, ownership identities, event ordering and measured costs with each
failure. The deterministic reproduction must exercise the production owner and fail
on the captured invariant before a correction can count as a repair. Correct that
owner's algorithm and delete superseded decisions at callers; do not duplicate the
algorithm in a simulator or add a leaf workaround. Validate safety, progress and
relevant event interleavings, retaining the reproducer as a regression test. Finally,
verify the same invariant and useful-work performance in the real system. Simulation
predictions and live measurements remain distinct evidence. Any new discrepancy
starts the cycle again; a green test or successful deployment does not close it.

### Coverage audit and deterministic execution

Joel asked on 2026-11-06 whether the prior failures can be simulated. Yes: extend the existing
dyno and owner tests, not a separately implemented version of the desired algorithm. Each scenario
records its production revision, source receipts, event schedule, asserted invariant and scope.
Classify results as production-policy replay, actual transport/owner integration, missing test seam,
or real-engine/live measurement. A green result in one category is not proof of the others.

Direct run at local `f420b8a490`, 2026-11-06 15:25Z:
`cargo test -p continuum-engine --lib capacity::dyno:: -j 2 -- --include-ignored`.
Seven tests executed in under one second after cached compilation: four passed, three failed.
Two failures execute the production grant policy against recorded demand and leave the waiter
unserved across 28 holder completions. The third is an intentional missing-view-seam panic, not
a simulation of the view algorithm. The assertions' old next-completion bound must be reconciled
with section 4's feasible-opportunity contract; this replay does not exercise the complete live
dispatcher/custodian. Do not relabel these failures as a new live outage or simply weaken assertions.
Log: `~/.continuum/evidence/2026-11-05-recovery/dyno-including-known-red-1525.log`.

The grant reservation owner's nine tests separately pass (15:26Z), covering exact lease identity,
retirement, terminal receipts, dropped streams and replacement-engine reservation. This establishes
those tested owner transitions, not end-to-end fairness. Log: `grant-owner-tests-1526.log` in the
same evidence directory. This distinction is why the next validator must compose actual owners.

Live-to-reproduction loop (2026-11-06 15:35Z): the fresh Windows capture
`windows-live-1535.log` records Sahar's unchanged-prompt resize at 15:28:56Z.
Exact prompt 18,702 plus predicted reply 3,769 exceeds admitted window 22,435;
reply 3,732 fits the strict boundary. The logged common prefix is 100%, and the
second exact quote takes 49 ms. Outer lane wait is zero; the approximately twelve
seconds before that quote cannot be attributed to outer queueing without tracing
paging and inner admission. A test-only extension of the existing Kimi HTTP fixture
uses these recorded counts with a synthetic prompt/backend. It is a reproduction
of the transport refusal/retry seam, not a full prompt, custody or latency replay.
Its compile/test result must be recorded separately before counting it as coverage.
This is the required iteration: observe real work, isolate the causal transition,
reproduce it through the production owner, change the algorithm, and verify real work.
Build deterministic scenarios by advancing a virtual clock to the next event, not sleeping.
Feed the production decision code recorded demands, service curves and completion identities.
Control the actuator with scripted quote/save/restore/send completions; enumerate bounded event
interleavings and use seeded generated schedules, retaining the shortest failing trace. Assert
conservation, legal state transitions and ownership after every event, not only at the end.
Include duplicate/late receipts, cancellation at each phase, same-URL engine replacement, queued
lifecycle writers, all-pinned waits, saturated submission, and grant revocation during quote.
Progress assertions name their assumptions (feasible demand, finite prior work and responsive
backend). Performance predictions use measured costs with explicit uncertainty; real-engine tests
verify the costs and pipeline overlap rather than treating virtual time as measured acceleration.

No acceptance run may silently exclude known-red scenarios relevant to its changed contract.
Publish passed, failed, ignored and missing-seam counts separately. Replace a placeholder with a
real production-path reproduction before claiming it repaired. Expand the existing actual-HTTP
Kimi case into the new dispatch contract's one-authorized-send test and preserve invalidation tests.
The complete simulator remains unfinished; this audit is the starting coverage inventory.

A seconds-scale validator in CI and on any machine, built from the REAL policy code (launch sizing,
the admission decision, the grant policy, the page custodian's contract, the view codec) driven by
recorded inputs (the day's memory readings at each boot, per-act prompt items, the measured
prefill/decode curves above). Existing pieces it builds on: `capacity/window_sim.rs`,
`vdd::turn_replay`, the serving_daemon fixtures, the Epoch.

It must first REPRODUCE each 2026-11-04 failure (red), then show the design removes it (green):

1. cold boot with the old engine still wired -> no launch until Settled; pool = reserved fit;
2. four minds oversubscribed on a small pool -> every mind served within the stated bound;
3. a turn completes and restarts between ticks -> the waiter is still served;
4. cancellation during prefill -> custody held until the engine's completion or a fenced exit;
5. a delayed page receipt -> no slot reuse until verified;
6. a restart -> pages restored, no five-minute cold re-prefill for a warm mind;
7. a reducible overflowing view -> older items collapse to handles while required/newest content
   remains whole; an irreducible shortfall requests admitted capacity/placement or explicitly waits.
   Neither blanket growth nor a blanket ban on growth satisfies the contract.

Asserted every run: physical bytes conserved (weights + resident KV + staging <= pool); no hold
without a releaser; eventual service for every mind; predicted acts/hour per resident from the
measured prefill rate. The dyno proves state and accounting (conservation, custody, service, ordering) and compares
PREDICTED performance; live hardware validates actual performance. A live change ships only when the dyno is green for it, then one change per
deploy with a 15-minute read of useful work and latency components. Joel's 2026-11-06 direction
supersedes activity-based rollback: use failures to correct the current architecture. Preserve
state and separate measured safety failures from low activity or slow execution.

## Order of execution

| # | Slice | Owner | Gate |
|---|---|---|---|
| 0 | Reconcile this design and shared interfaces against current evidence and active work; current architecture is the baseline | Fable + Codex + Cormac | recorded review of owners, invariants and deletion plan; no claim of signatures not received |
| 1 | The dyno with the seven reproductions (red) | Fable + Codex | each 11-04 failure reproduced from recorded inputs |
| 1b | The fast path audit: every cold turn classified 3C (refused restores, unconfirmed pages, scratch-slot churn) (page refused, page unconfirmed, prefix lost); geometry stable across restarts; pages eligible by full compatibility and complete-save provenance | Codex + Fable | dyno 6 green; live: cold turns after a restart = 0 for a warm mind |
| 2 | The view: reads by region, side budgets into the one allocation run once, history to handles; revert the wish raise of #5773 | Fable | dyno 7 green; prompt tokens per act at the working set |
| 3 | Engine memory (after the 1b diagnosis): launch-from-fit at Settled (#5770 first), conservation accounting; then delete growth (#5774) | Codex + Fable | dyno 1, 6 green |
| 4 | Custody and cancellation first: #5772, fork prefill cancellation; the page custodian's surrender/verify contract | Codex | dyno 4, 5 green |
| 4b | Bounded decode latency under mixed load: chunked prefill from measured chunk time, measured cold-prefill admission limit, cancellation at chunk boundaries, recurrent state intact (after the paging outcome slice) | Codex (fork) | re-run Cormac's mixed trial: decoder first/inter-token latency bounded AND every prefill completes |
| 4c | The pipeline (5e): encoders record, the dispatcher executes double-buffered frames by sort key; tool results prefill as they stream; page-in ahead; write-back behind | Fable (core encoders) + Codex (dispatcher) | GPU idle fraction while minds wait, before/after; per-act split tool/wait/prefill/decode |
| 5 | Admission: one decision over residency, prefill, decode; fairness by completion identity; rotation THROUGH the proven custodian | Codex | dyno 2, 3 green |
| 6 | Her state: command preconditions, results with next moves, Shown ids, one repeat owner | Fable + Cormac | precondition tests; live: no verb hunted (`commands/help` loops) |
| 6b | The tool contract: payload-sized output room + continuation of a cut call; content-anchored edits with read hashes (line modes deleted); results only through the codec; typed refusals; schema validation before execution | Fable (contract, cognition) + Codex (fork continuation) + Cormac (typed refusals) | live: landed calls per attempted call per mind; ratchets at 0 |
| 6c | One protocol rendered from types: VerbRef + wire names, typed affordances, one silence/review protocol, typed causal outcomes, descriptions from declarations, prose ratcheted to 0 | Fable (renderer, VerbRef, affordances, facts, headers, descriptions) + Cormac (typed refusals, CommandError causes) + Codex (review) | audit replay: every verb named is in her function list; prose-literal ratchet at 0 |
| 7 | Learning as a dream stage; persistence on change; deploy custody | Cormac | slowdown measured within its bound |

## Open branches, fitted

| Branch / PR | Fits | Disposition |
|---|---|---|
| #5769 Probed | stage 5 (typed "never asked") | keep, merge after review |
| #5770 adopted-plan credit | stage 5 launch | merge first in slice 3 |
| `fable/launch-from-fit-pool` | stage 5 launch | slice 3, after #5770 |
| #5774 growth deletion | stage 5 | blocked until launch-from-fit is proven |
| #5772 busy is not dead | stage 5 custody | slice 5, after its tests pass in CI |
| `fable/grant-never-starves` | stage 4 | rework: rotation through the page custodian; one decision with stage 4 |
| `fable/state-driven-surface` | stage 1, 6 | slice 6 |
| #5775 Shown ids | stage 6 | merge (reviewed) |
| #5773 (merged) | stage 3 | keep the typed measurement; revert the wish raise in slice 2 |

## Superseded (marked in the files, removed from "read first")

INFERENCE-LANES-REALISTIC, INFERENCE-SCHEDULING-AND-SCARCITY (slot pools, adaptive quantizer),
KV-CACHE-ECONOMY:28 (`-c = window x lanes`), MINDS-SEATS-AND-ADMISSION:62 (seat = lane),
CONTEXT-IS-A-CAPABILITY-AXIS:44-66 (lane count), CLAUDE.md 3b's "in-process", ONE-BUDGET 3b's
"flex_allocate deleted", SERVING-ENGINE §1c's "done" for lane-free deliberation (not done),
planning/AI-LANE-OPEN-QUESTIONS and UNCERTAINTY-IS-THE-SCHEDULER-SIGNAL's "N lanes",
PHASE3B-WORKING-MEMORY-PLAN's RAG token budget.

## Agreed

- Fable: (pending)
- Codex: direction agreed (2026-11-04); signature pending these corrections, now folded in
- Cormac: stages 6 and 7 and the cancellation ruling agreed (2026-11-04), with his two receipt corrections applied; rest pending

## Takeover evidence receipt — 2026-11-05 15:15Z

The shared implementation status is now: slice 2 (#5779) merged as `631502fcf6`,
lease/output admission (#5787) as `d04fe4594d`, and typed tool calls (#5790) as
`9a0f610d5b`. All checks passed on #5790's integrated head `9ef326dda9` before merge.
Binary publication is still running; this is CI evidence, not installed or live proof.
IntelMac directly answered `ping` with `147694174`; M5 with `2069cb248`.

The admission validator itself has not been recovered or completed. Before constructing it,
Codex recovered the full [IntelMac evidence](https://gist.github.com/joelteply/90f98f20980705433573ad32d68aad1c)
and the M5 SSE arrival records, and executed an independent arithmetic check:

| IntelMac active cold requests | Aggregate prompt tokens | Wall seconds | Aggregate tokens/s |
|---|---:|---:|---:|
| 1 | 4,641 | 67.946 | 68.304 |
| 2 | 9,282 | 136.450 | 68.025 |
| 4 | 18,564 | 293.819 | 63.182 |
| 9 | 41,769 | 681.234 | 61.314 |

Every rate reproduced `sum(prompt_n) / wall_s`; every trial had the expected request
count and no recorded request error. One is the maximum observed throughput concurrency;
these single trials do not establish a statistical knee. Keep this CPU curve node-specific.

For the M5 warm-first pair, three trials per chunk size reproduced these measurements:

| Prefill chunk tokens | Largest observed SSE content gap, seconds | Median warm decode tokens/s | Median cold completion, seconds |
|---|---:|---:|---:|
| 512 | 0.756 | 41.828 | 21.713 |
| 1,024 | 1.212 | 44.859 | 19.036 |
| 2,048 | 2.236 | 32.710 | 18.362 |

Raw M5 source: `~/.continuum/evidence/codex-harness-20261104/chunk/b{512,1024,2048}`.
These are SSE content gaps, not measured engine scheduling boundaries or proven upper
bounds. A content chunk can contain several tokens. They do not validate cold-first or
four/nine cold-arrival traces. The simulator must keep those missing cases explicit,
separate the cold-concurrency policy invariant from latency/fairness/completion gates,
and validate against the recorded hour before predicting other workloads.

Reproduction and copied evidence on the takeover host:
`C:/Users/joelt/.continuum/evidence/2026-11-05-recovery/check-admission-evidence.py`.
Its outputs are `admission-curve-validation.json` and `m5-chunk-validation.json`.
No live setting was changed to obtain these results.

## Live rollout and rollback receipt — 2026-11-05 16:00Z

M5 installed `d04fe4594` through the supported graceful reboot at 15:28Z,
with the existing engine process preserved. The 15:28–15:43 observation had
7 completed acts (Aris 5, Benchy 2, Aiko 0, Atlas 0). The available baseline
had 12 (Aris 4, Benchy 2, Aiko 3, Atlas 3), covering 15:10:14–15:22:38;
log rotation shortened the requested baseline. This failed the live acceptance
gate despite green CI. These unequal, uncontrolled windows do not isolate which
change caused the regression, and cold-cache transition costs remain a confounder.

The supported rollback restored core and CLI `2069cb248`, verified by ping and
version, with the same engine PID 25678. All 66 modules reported durable shutdown.
By 16:00Z, the still-open rollback window recorded 16 acts across all four minds
(Aris 4, Benchy 9, Atlas 2, Aiko 1). This is evidence of resumed progress, not a
speedup estimate or proof of productive writes. Keep completion, useful work,
cache reuse and custody as separate outcome measurements. A bounded one-hour
M5 deployment hold protects observation until 16:50:51Z; it must be reviewed.

IntelMac independently auto-adopted `d04fe4594` around 15:31Z. Its 15:32–15:47
window recorded no completed acts. At 15:58Z multiple advancing slots reached
1500-second deadlines and received extensions. That is saturation evidence,
not a dead-engine diagnosis or a reason to discard pages. Intel's baseline and
rollback decision require separate evaluation; M5 recovery does not prove it.

Private raw logs, comparison script and rollback receipts remain in the takeover
host's `2026-11-05-recovery` evidence directory and M5's
`~/.continuum/evidence/2026-11-05-takeover/d04fe4594dbf` directory. Full resident
logs are not published with this document. The dispatcher/validator is still
unfinished; these live outcomes do not validate the proposed graphics-style scheduler.

### Follow-up live receipt — 2026-11-05 16:41Z

The complete M5 rollback window (15:48–16:03) contains 21 completed acts:
Aris 5, Benchy 10, Atlas 4, Aiko 2. Current and rotated logs were combined and
deduplicated; coverage is 15:48:00–16:02:59. The earlier 16-act figure was interim.
The M5 remains on `2069cb248`; its bounded deployment hold now expires 17:50:51Z.

Intel's attempted rollback to `147694174` did not pass acceptance. Its runtime
replaced the engine and changed geometry from nine slots to two, then five.
An `inference.slot_affinity.all_pinned` retry storm then rotated the entire log
history rapidly. Missing historical completions therefore cannot be counted as
zero. The notification source has not been isolated; no speculative sleep or
logging-suppression patch has been applied.

The supported reversal to the saved `d04fe4594` completed after a 302-second drain
and 22-second startup, reporting all 66 modules durable. By 16:40Z, core ping
verified d04 and the sampled last 10,000 log lines contained no all-pinned events.
The runtime had relaunched a nine-slot engine (PID 25045, pool 294912). This is
containment of the retry storm, not evidence that productive-act latency is fixed.
Earlier claims that Intel's engine remained unchanged are explicitly withdrawn.
The subsequent live observation remains open; no further rollout is accepted.

### Engineering recovery — 2026-11-05 17:43Z

Intel's full reversal window (16:36:50–16:51:50) contained only one
`reached_model=true` outcome (Eitan, 535 seconds). Nine other pace records
were aborted attempts with `reached_model=false`, not completed model work.
The live gate failed despite containment of the retry storm. M5's 21 rollback
outcomes were checked and all reached the model; neither count establishes
useful writes or a controlled causal speed improvement.

PR #5792, head `2f4c33382a`, fixes a demonstrated notification defect: releasing
a reserved scratch slot broadcast to citizen waiters although no citizen slot
became available. The regression failed with one unexpected wake before the
fix; nine contention tests and 38 admission tests pass afterward. Both permit
acquisition paths now use one constructor; scratch releases retain their own
semaphore wake without broadcasting citizen availability. Engine CI passed;
full CI and live acceptance remain pending. This does not establish the source
of the entire Intel retry storm.

Fable's original dyno PR #5777 was recovered and integrated with canary `9a0`
at `8c25a1e160`, preserving the newer stamped-admission and custody tests.
Four recorded dyno checks pass (three future gates remain explicitly ignored),
and admission integration passes 37 tests with six ignored. Running the ignored
whole-pool fairness gate explicitly still fails: Aris receives no grant by the
holder's first completed turn, with the last grant remaining `{1: 114592}`.
This is a reproduced production grant-policy failure using recorded inputs,
not proof of a new scheduler. CI is pending for the recovered branch.

The measured 5c/5d admission validator remains unfinished. The older contention
replay assumes an equal prefill/decode split and a constant prefill rate; it
cannot certify nonpreemptible chunk boundaries or measured warm-decode gaps.

### Handoff implementation checkpoint — 2026-11-05 18:24Z

The recovered dyno passed all CI and merged as `24e31fb919`. The scratch-slot
wake fix passed all CI and merged as `f5bcda1dd5`; its platform publication
`42078670563` is running. Neither milestone establishes live speed gains.

Fairness work is isolated on `codex/grant-receipt-fairness`. Commit `a078faf257`
records terminal receipt revisions under the same mutex as fixed reservations.
Seventeen grant tests pass: progress survives completion and restart between
publisher ticks; unsent retries and cancellation do not count as service; a
fenced engine's late receipt cannot resurrect progress. This is implemented
custody bookkeeping, not yet a serving-policy change.

The next phase fences a completed holder's reacquisition while continuing to
charge its resident capacity. The existing re-home path keeps a binding when
its grant disappears, and the shared engine deliberately retains idle KV.
Therefore deleting a grant is not a reclamation receipt. The pending handoff
must not release its fixed claim until verified save-and-erase or an engine
fence. The pinned engine fork exposes `/slots/{id}?action=erase`; production
integration must validate its response, protect page IO and endpoint identity,
and retain custody across cancellation before enabling the new policy.

### Reclamation and publication checkpoint — 2026-11-05 19:07Z

The local fairness branch now implements endpoint-retained save then erase,
with endpoint identity, slot permit, page IO and per-persona custody held across
the operation. A confirmed save remains on disk; erase proof is recorded with
the reservation owner before notifying the caller. Uncertainty keeps the claim
charged. Existing fork pages refuse resident-only reclamation. Late unresolved
completion recovery remains unfinished, so this mechanism is not enabled yet.

The new publication transaction consumes physical proof and publishes the next
queued grant while holding the dispatch reservation mutex. A completed holder
joins behind existing waiters. A regression exposed a stale service-loop
`in_flight` flag re-admitting the old holder even after erase; the transaction
now derives in-flight protection from actual reservations. The regression
failed before that correction and passes after it. Twenty-two grant tests pass,
including cancellation protection, publication lock ownership, and three-mind
rotation; the older ignored policy gate still documents the unwired path.
The physical HTTP tests separately cover save-before-erase and save refusal.
These are local component tests, not an end-to-end serving or live fairness proof.

Serving integration still needs the pending wait path, known-refusal recovery
and late completion recovery before enabling handoff. The scratch wake fix
`f5bcda1dd5bf` has now published successfully on all three platforms in run
`42078670563`; it has not been deployed by this takeover yet.

At 19:17Z, local commit `cb16505bb1` closes the late-receipt continuation gap:
the existing retained page owner returns custody only for a verified terminal
reply; malformed replies quarantine before releasing custody. The retained
handoff task resumes save/erase on that result, even if its caller leaves.
Six unresolved-page tests, 42 admission tests (six ignored) and both physical
handoff HTTP tests pass. Known-refusal recovery and serving integration remain.
In particular, completion must bar reacquisition when another mind is owed
capacity; polling for a brief idle interval is insufficient. No live deployment
or end-to-end fairness claim follows from these component checks.

At 19:22Z, the reservation owner also implements the completion boundary:
publication arms one holder when a waiter is owed capacity, and compare-and-reserve
refuses reacquisition once that holder's terminal revision advances. The next
publisher tick can therefore start reclamation without having to sample an idle
gap. Capacity remains charged throughout. Twenty-three grant tests pass, including
no intervening tick, unsent attempts, overlapping unresolved custody and withdrawal
of the waiter. Known-refusal recovery and serving integration are still pending;
this is local implementation evidence, not live acceptance.

At 19:35Z, known-refusal recovery returns an owned ticket to the reservation
owner, then restores the holder's charged grant atomically at publication.
It never supplies capacity to a waiter. Preparation now runs inside the retained
task, so dropping the caller cannot abandon the ticket before page IO begins.
Uncertainty still retains the claim; only a verified reclaim receipt frees it.
Twenty-four grant tests pass, including restoration without immediate retry;
three HTTP handoff tests cover successful save/erase and refusal of either action.
Serving integration and its wait path remain unconnected. Resident-only reclaim
still refuses existing fork pages, so this is not proof of fairness for that case.

At 19:48Z, serving publication is connected to persistent `FairGrantState` and
endpoint-retained reclamation. `mind_window` treats a due handoff as ungranted;
the page-user gate parks on the existing grant watch without holding a lane or
page custody. The serving crate passes `cargo check`; two mind-gate tests and
24 grant tests pass. The new wait test covers an unrelated publication and
verifies that the physical reclaimer can acquire custody while the mind waits.
This is an integration checkpoint, not rollout acceptance: the combined HTTP
handoff-to-publication test, current-canary integration and CI are still required.

At 19:57Z, draft PR [#5793](https://github.com/CambrianTech/continuum/pull/5793)
contains the connected implementation at `c3fb72c7e1`, merged with current canary
`24e31fb919` including the scratch wake fix. A combined regression runs production
preparation and HTTP save/erase, records the real validated proof with the owner,
then publishes the waiter: no early free, saved page preserved, old lease refused.
It replaces the ignored terminal-only gate, which incorrectly omitted physical
reclamation. On this integrated head, 25 grant tests pass (none ignored), 45
admission tests pass (six existing ignored), 39 slot tests pass, and the serving
crate check passes. Exact-head CI is running. The PR remains draft and undeployed;
fork-page limitations and measured scheduling validation remain as stated above.

At 20:06Z, CI's source-hygiene failure was corrected: standalone test modules
now declare file-level `cfg(test)`, and the initial receipt baseline has its
required inline justification. The full local base-versus-head hygiene run passes
87 checks on each tree; CI hygiene also passed on `a9a12c613a`. Runtime CI is
still pending. Review additionally pinned conservative erase-refusal behavior:
the fork can reject an invalid slot before mutation, so rejection is not proof
of warm residency. The saved page and charged claim remain, but attribution stays
unconfirmed. A targeted regression passes; no production shortcut was introduced.

At 20:14Z, review reproduced a multi-holder fairness failure: with holders at
20 and 60 tokens in a 100-token pool, a 90-token waiter lost the first reclaimed
cells to the smaller holder's re-admission. Publication now withholds new queued
admissions when the oldest eligible waiter still needs another holder reclaimed;
existing held and unresolved claims stay protected. A waiter whose need no longer
fits the pool/ceiling is removed from that priority queue. The new regression
fails before the correction and passes after it; all 26 grant tests pass,
including the combined HTTP proof test. CI must validate this newer PR head.

At 20:30Z, every applicable CI check passed on exact head `44fe91a0c0`,
including engine, serving, core, source hygiene, Windows compilation and binding
drift. PR #5793 was marked ready and squash-merged into canary as
`584c987fd016e2231e0d3348644861472c9e96e8`. Binary publication run
`42103921846` was dispatched behind active run `42084725475`; the active
publication was preserved. This is CI acceptance, not live acceptance. Deploy
only the verified artifact through the supported lifecycle, with baseline,
rollback and a 15-minute resident outcome gate. Fork-page reclaim and measured
5c/5d scheduling validation remain unfinished.

At 21:23Z, recovered slice-3 pool-growth regression was run explicitly and failed:
a declined relaunch left both its request and admission hold live. PR #5794
(`e5cd0745e7`) settles an equal/smaller target as a known refusal, allowing the
existing post-ensure owner to reopen admission. Unknown readback remains pending.
Both owner tests pass (none ignored), with shared-owner test isolation. CI and
live validation remain pending. The M5's supported 584c deployment was deferred
before stopping because its lifecycle gate is busy; the old 2069 slot was restored
and verified. The code defect is reproduced; it is not yet proof of the complete
live gate's cause. No forced restart or page deletion was performed.

At 22:12Z, PR #5794 passed all applicable CI and merged as
`8b964d5d58d1af236f47deaa60a6308ebebe6553`. Publication run `42119014603`
is the source for the common three-node deployment target, containing both
fairness and declined-growth settlement. An isolated deployment checkout is
prepared; the stale Windows checkout is not a deployment source. Acceptance
requires this validated revision running on Windows, M5 and IntelMac with live
useful resident work, saved-page continuity and bounded waits. Neither CI nor
historical rollback activity establishes that result.

At 2026-11-06 02:04Z, the three-node acceptance gate remains open. All three
machines adopted 8b through the supported lifecycle. M5's complete 22:50–23:05Z
window recorded 18 model-reached acts across all four residents and one confirmed
code edit. Intel's first window recorded one model-reached act; its long admission
and physical-slot waits still need correction. These are different outcomes, not
a blanket recovery or causal throughput claim.

Windows crashed at 00:18:14Z after concurrent embedding contexts requested roughly
1201 MiB compute buffers and a native allocation exception crossed Rust FFI.
PR #5795, exact head `545b91f366c13ba401b4ffda569b838202cf8b7e`, retains embedding
permits and memory claims inside blocking workers, serializes contexts per backend,
and contains native decode exceptions on the C++ side. Embedding graphs consume
full inputs within the model limit instead of silently clipping at 2048 tokens.
Generation consumes an explicit nonzero allocation; private RAM probing and
fraction/floor sizing are deleted. Scheduler allocation identity is retained.

Local cancellation and queue tests, native exception injection, four concurrent
real-model embeddings and a 2110-token suffix-sensitivity regression passed.
The short-input compute buffer measured 5.42 MiB instead of 1200.97 MiB; this is
input-specific evidence, not a universal footprint bound. Final backend/adapter
checks passed 17 tests with four explicit-environment tests ignored, and source
hygiene passed 87 checks. Exact-head CI is still running. Existing embedding memory
accounting is not a completed ONE-BUDGET migration. The repair is not deployed,
and Windows has not been blindly restarted after the crash. Preserve the separate
serving engine, resident identities and saved pages during supported recovery.

At 2026-11-06 02:14Z, PR #5795 passed every applicable check on exact
`545b91f366c13ba401b4ffda569b838202cf8b7e`, including engine, core, source hygiene,
Windows library/tests compilation and binding drift. It merged as
`5d5985c1c209de99791400cd029d4286a11b8ecb`. Supported binary publication
`42149032888` targets that exact merge. This is CI acceptance only; no repair
artifact has been deployed and Windows recovery is not yet established.

### 2026-11-06 03:15Z: repair rollout, live gate still open

M5 core and CLI now identify `5d5985c1c` (build 6817), deployed through the
supported reboot at 03:06:32Z after exact archive/manifest/checksum and validate-only
checks. All 66 modules saved; the dark window was 22 seconds. The engine was
unchanged at immediate verification, but runtime pool growth relaunched it at
03:06:44Z as PID 6967 with five lanes and requested context 547340. Therefore this
is not an unchanged-engine comparison. Early evidence through 03:13 contains one
model-reached Aiko act and three aborted attempts during that transition. The full
03:06:32–03:21:32 gate is pending; this is not productive acceptance.

Intel's matching artifact passed verification and validation; a supported reboot
is draining in-flight work as of 03:15. Windows publication is still building and
the crashed Windows core has not been restarted. Neither CI nor partial publication
satisfies the three-machine goal. Baselines, rollback binaries and hashes are kept
in each Mac's `~/.continuum/evidence/2026-11-05-takeover/5d5985c1c209` directory.

### 2026-11-06 03:26Z: M5 gate failed; Windows repair activation underway

Full rotated-log coverage for 03:06:32–03:21:32 contains seven model-reached acts
(Aiko four, Aris two, Benchy one, Atlas zero) and three aborted engine-transition
attempts. The immediately preceding 02:50–03:05 window contains 17 model-reached
acts across all four minds. This fails the rollout's productive gate; automatic
engine growth/relaunch confounds attribution, so it does not establish that the
embedding custody fix alone caused the decline. A supported restoration of the
saved 8b core/CLI is draining on M5; this is containment, not completion of the
current-code goal. Evidence: m5-5d-summary.json and m5-5d-baseline-summary.json.

Intel core and CLI verified 5d at 03:20:13Z after a 320-second dark window. Runtime
relaunched engine PID36340 (five lanes, context163840); its live gate ends03:35:13.
Windows publication is complete. PrepareOnly verified exact5d and staged service-b;
ResumePrepared activation is in progress. No Windows recovery claim precedes its
live resident evidence.

### 2026-11-06 03:46Z: completed observation windows

Windows 5d survived the prior crash window and restored Kimi and Sahar from disk.
The full 03:26–03:41 window contains two model-reached Sahar acts (50 and 85 seconds),
none for Kimi, and no false attempts. This is crash-containment progress, not
productive acceptance for both residents. Intel's full 03:20:13–03:35:13 window
contains one Delia act (417 seconds) and one false attempt; it also lacks productive
acceptance. Keep current code while repairing the demonstrated admission problem;
these Intel results do not establish a decline from its already poor 8b baseline.

M5's saved-8b restoration completed at03:28:07. Its full following 15 minutes
contains ten model-reached acts across all four minds, no false attempts, versus
seven and no Atlas completion on5d. This remains below the predeployment17.
The larger engine geometry survived restoration, so none of these windows isolates
an embedding-code effect. No further rollback cycle is the engineering solution.
Stable summaries and rotated logs are retained in the recovery evidence directory.

### 2026-11-06: caller lane custody reaches physical admission

Local branch `codex/outer-lane-custody` consolidates cognition's serving-lane
permit into `TurnAdmission`. A request parked on all-pinned KV relinquishes the
caller lane and endpoint, then reacquires through the same scheduling authority,
persona, priority and occupancy estimate. The queued mind acquisition is preserved
when it waits, while its caller lane is released. Review reproduced that second
retention defect before correcting it; neither fix discards page custody.

The actual `generate_admitted` HTTP test demonstrates lane lending during a
physical wait, no dispatch while another owner holds the lane, one dispatch after
reacquisition, and release on the fixture's terminal refusal. Local validation is
47 admission tests passed (six existing ignored), three scope-ownership tests,
and the HTTP integration. Current canary 5d is merged into the branch. Final
hygiene/cognition check and exact-head CI precede acceptance. No live deployment
or complete Intel root-cause proof is claimed. Task-local custody scopes the
directly awaited provider future; detached work receives no implicit authority.

### 2026-11-06: operator correction — current code is the engineering target

Joel explicitly rejected treating old-binary activity as recovery. Keep all three
machines on current validated releases and repair failures there. An activity-count
decline, particularly across an automatic engine geometry change, does not justify
cycling back to an older core. Preserve state and stop concrete data loss; historical
old-build success is not an acceptance criterion.

M5 is being returned through the supported lifecycle to the published, all-CI-green
5d release already running on Windows and Intel. The fresh evidence is under
`5d5985c1c209/current-0524` on M5. PR5796 remains separate and unmerged until its
exact-head CI completes; its isolated genome fixture rerun is in progress.

PR5796 passed all applicable CI at exact head827836d83c, including the unchanged genome fixture on its targeted rerun. It merged as canary49e0fadee47709366ff411393c7e7c80d125ed0c. Supported publisher42169216864 names that exact revision; publication and live verification remain pending. All three machines were directly verified on published5d at05:26Z, with M5's fresh observation window ending05:40:10Z. Current-code failures are engineering evidence, not justification for activity-based rollback.

M5 current5d window05:25:10–05:40:10 has full rotated coverage: seven model-reached outcomes (Aris2, Aiko5), no false attempts; one confirmed code edit and published review/submission receipts. Atlas and Benchy had no completed outcome in that window, but both were admitted with measured advancing prefill (Atlas13,290/62,884 with zero cached; Benchy41,439/49,264 with39,303 cached). This is current-code performance evidence, not a rollback trigger or proof that lane custody alone explains their long turns. Raw logs and summary are in m5-current-0525. Publication42169216864 for49e0 remains in progress.

M5 now runs lane-custody release49e0fadee, core and CLI build6818 verified05:57:04Z. Exact manifest/platform/archive checksum and validate-only passed; supported reboot saved66modules with21s dark window. Supervisor PID67455 healthy. Engine PID/start/argv matched immediately before/after only. Observe05:57:04–06:12:04 for useful acts, pages and waits before acceptance; no activity-based rollback. Intel and Windows remain5d while their49e0 artifacts build. Remote49e0fadee477 retains baseline, recovery binaries, validation and deployment receipts. This is deployment evidence, not proof of productive recovery across all three nodes.

M5's complete49e0 window05:57:04–06:12:04 has10model-reached turns (Aiko5, Aris3, Atlas2), two unconfirmed attempts during the automatic engine transition, and one confirmed Aris code edit. Benchy was admitted with advancing prefill but no completed turn in this window. Atlas's725-second turn records723111ms model time and2292ms residue; a later turn took53seconds. This does not prove the lane fix solves model-path latency or all-resident productivity. Keep current code and investigate measured prefill/reuse; do not infer a new admission defect from duration alone. Evidence: m5-49e0-gate/summary.json and outcomes.txt. Intel/Windows publication remains pending.

Windows is now on49e0 core/CLI6818, supported installer completed with66modules durable and99s dark window. Its06:48–07:03 full gate records Kimi2 and Sahar1 model-reached turns, no false attempts, and no confirmed write. Intel deployment was refused before stopping because Delia training9e0106d6-ed93-4a8c-af40-7da49eb1683f retains engine36340. Direct engine status advances12→14→18 training windows while epoch0 remains incomplete; preserve custody and require a terminal receipt before handoff. The Intel engine identifies08d6d354b:cpu; its server-train.cpp is unchanged against inspected0d7dda. This is paced chunk work, not demonstrated deadlock. No activity-based rollback; all-three current productive acceptance remains outstanding.

Intel training now has a completed epoch0 receipt: train_loss6.938771, eval_lossnull, duration8871.788seconds. The exact job's engine log specifies three epochs; it remains running with no saved-adapter receipt. This proves an epoch completed, not useful learning or readiness to deploy. Correct the earlier shorthand: native windows increments in before_window after governor admission and before the work, so it counts admitted windows, not completed optimizer steps. Increasing measured busy time corroborates execution; only the terminal owner receipt releases handoff. Evidence: intel-training-first-epoch.txt and intel-training-progress-semantics.txt. Preserve current49 on Windows/M5 while the existing Intel job finishes; do not force, mint budget, or equate epoch0 progress with all-three recovery.

At10:43Z Intel completed epoch1 as well: train_loss6.885515, eval_lossnull,6459.249seconds. Three epochs were requested; the exact job remains running, so neither adapter quality nor terminal custody release is established. Preserve the job.

PR5798 passed all applicable CI at dbbbced9d1b702601047a01acb6fab712507da44 and merged as ed142f14c21af335c606ed5fba68542c1bba8974. View now reports actual restored_items/restored_chars through the existing turnover probe, sharing the decision's held-item promotion predicate. Tests distinguish new items, folded-to-pointer changes, failed turnover, repeated restoration, and pointer/summary/full promotion. This is instrumentation, not a turnover-policy or performance fix. Atlas's historical fitting-view restoration preceded40,632fresh tokens; old aggregate receipts cannot quantify its restored items. Publisher42204975211 names exacted142; no deployment or live benefit is claimed.

At13:54Z all three nodes directly answer ed142/build6819, with matching CLIs verified at deployment. Windows used PrepareOnly/ResumePrepared, M5 and Intel used supported prebuilt reboot. Intel's training completed all three epochs and saved its 4,365,792-byte adapter into Delia's genome before handoff; the terminal receipt and adapter hash are retained in ed142f14c21a/retry-1346. This establishes durable training completion, not learning quality. Intel saved66modules, reported164seconds dark window, and runs under gui/501/com.continuum.core. Its engine automatically relaunched at13:48:27; do not claim engine preservation.

Completed ed142 live windows remain insufficient for productive acceptance: M5 had9model-reached outcomes, Windows10(all Sahar), with no confirmed writes in either window. Intel's13:47:33–14:02:33 window is still pending. Preserve current code and repair observed causes. Kimi's first Windows turn completed after the gate with0cached/27,221fresh tokens; corrective attempts waited156.5seconds and24.6seconds for admission. The final token quote took67ms, so counting alone does not explain the surrounding latency.

Kimi's oversized reply cap is reproducible without weakening capacity: prior density30846/31801 prices the next estimated27479tokens at26654, leaving5857reply tokens under32512. The selected engine counts27221, leaving5290. Existing guards correctly refuse the larger sum and the caller retries. Local actual-HTTP validator3383da2c51 passes1test: the oversized request never generates, the resized request is quoted again, and one unchanged prompt is sent. It is a baseline reproduction, not an adaptive-room fix, lease proof, or live performance claim. Any optimization must carry explicit act reply-room authority and minimum, update the wire and receipts together, and retain per-attempt engine/lease validation. Kimi's existing4b1e245c work owns remembered steady boot geometry, a separate scope.

Intel completed its ed142 gate13:47:33–14:02:33 with full rotated coverage: one model-reached Eitan turn (441seconds), three unconfirmed attempts at the automatic engine transition, and no confirmed useful write. Eitan used0cached/16,479fresh tokens,413,342ms prefill,26,903ms decode and531ms residue. Other residents waited for pinned slots; queue estimates expired without removing their requests. All three current-code deployment gates are now complete, but productive recovery remains unproven. Preserve current code; these receipts drive fixes, not rollback or repeated gates.

### 2026-11-06 18:44Z: shared collaboration state

The first exact-output dispatch slice is committed and pushed as `eb59f22a9f`
on `origin/codex/quoted-act-room` (worktree `continuum-dyno-recovery`). It carries
explicit Fixed/UseAdmittedRoom intent, borrows the actual TurnAdmission, resolves
against owned physical/admitted geometry, and derives reservation, canonical wire
cap and mandatory sent receipt from one resolved value. Cognition consumes that
receipt and no longer retries an unchanged view merely to resize reply room.
Required-content refits remain. Canonical TypeScript and SDK wire copies are updated.

Evidence: 43 cognition and 57 engine integration tests pass, plus three schema
exports. Recorded Kimi/Sahar counts reproduce one quote and one generation with
matching cap and receipt through actual HTTP and real admission/grant owners;
the backend/prompt fixtures are synthetic. Windows development release preparation
passed and supported ResumePrepared is in flight. This is not yet live speed or
useful-action proof. Core/CLI/native CUDA builds are complete. An inactive engine
slot containing old TLS DLLs was archived intact after owner/receipt checks; the
supported staging retry passed. No live state or receipt was rewritten manually.

Work boundaries for incoming collaborators:

| Workstream | Deliverable and boundary |
| --- | --- |
| Dispatch review | Independently audit eb59f22a9f against sections 4a/5e; review immutable prompt, owned admission, canonical cap, result validation and removed retry. Do not implement a competing leaf resize policy. |
| Fair service simulation | Compose actual grant/paging owners with deterministic event interleavings. Reproduce the two existing unserved-waiter traces; define feasible service opportunities before changing fairness assertions. Preserve known failures as evidence. |
| Cold-prefill investigation | Capture live cache/paging and prompt-prefix evidence and reproduce the observed cold turns. Separate engine restart, changed grounding and restoration effects; no blanket cache or pool-growth changes. |
| Learned optimizer | Use the existing work package and policy boundary: reproducible baseline/corpus, held-out evaluation and shadow proposals. No alternate scheduler or ownership authority. |

The primary deployment owner continues the Windows handoff and measures actual
persona actions, quote/refit counts, queue time, cache reuse and useful outcomes.
Coordinate ownership before editing shared interfaces. Existing ambiguous-503 retry
and cancellation custody defects remain explicitly open; this slice does not claim
to repair them. Ratchet results also remain qualified: a substring scanner counts
PreparedCommand::new as a subprocess, and mechanical response migration touches
pre-existing heuristic-size and dreaming-required-input findings. Do not report
those checks as passing or expand unrelated cleanup into an iteration blockade.

Joel's development priority is fast evidence-driven iteration using available
compute and supported local builds/restarts. Uptime is not an acceptance gate.
Preserve durable state and others' unfinished work; routine deployment holds have
been removed. Live outcomes, not a green simulation alone, decide acceptance.

Live follow-up at18:52Z: Windows runs eb59f22a9f/build611 under the scheduled
supervisor (core41564; automatic engine868). Supported deployment saved66modules
and reported172seconds dark window. The first live defect is historical training
records failing with missing `dispatch`; parked examples remain on disk. The
correction belongs at persisted response decoding, shared by capture and LivedCall,
not a serde default on live responses. Explicit LegacyUnrecorded history must never
satisfy admitted-result validation. A deterministic parked-example reproduction and
strict-wire/preserved-receipt tests accompany this correction. Implementation and
consumer validation are in progress; it is not deployed yet. We remain collaborators:
primary continues deployment/live proof and this newly observed regression; other
workstreams can proceed independently within the contracts above.

19:33Z live verification: Windows8ea0f15239/build612 is running under its scheduled
supervisor. The historical examples that previously failed now decode and reach
`training.job.remainder_waits`: required windows33,536–35,840 versus served32,409.
This proves the storage regression is corrected, not that those examples trained.
The new action observation window remains open through19:40:30Z.

Prefill collaboration evidence: Claude's19:03 message (AIRC event466a07d3, original
retained locally) reports four concurrent cold prefills on M5 and poor resident
latency. Treat these as attributed measurements pending trace review. The current
`lane_args::UBATCH_TOKENS=2048` comment cites an August21 M5 isolated llama-bench
throughput measurement and failure to allocate4096 beside a serving lane. That
provenance does not establish a mixed-load optimum; the peer's November4 paired
warm-decode results test a different objective. The existing5c work must reconcile
these measurements through the shared operating-point owner and deterministic
mixed-load traces, not a new hardcoded launch flag or per-leaf policy. The exact
reply-room change does not claim to solve cold-prefill admission or fairness.

### General generation is not admitted output authority (CI reproduction, November6)

The real shared-teacher HTTP/custody validator found an omitted caller in the first
migration: teacher_generate intentionally omits max_tokens, but generic OpenAI
streaming was routed through Fixed. This refused before HTTP and produced an empty
teacher corpus. Local reproduction matches CI. Do not invent a teacher cap.

The centralized output intent must therefore also name ProviderControlled for
general generation. It preserves the original omitted-limit body through quote and
POST, checks prompt feasibility under the existing serving path, and returns
Unadmitted—not Counted or ProviderFixed. It cannot carry admission, a grant lease,
a think bound or a contradictory max_tokens, and generate_admitted must reject it
before invoking the provider. Fixed and UseAdmittedRoom keep their existing strict
contracts. A missing caller cap selects this explicit general case only at the
generic adapter entry point; it is never an admitted fallback. The actual teacher
validator must retain all existing shared-custody and cancellation assertions.
