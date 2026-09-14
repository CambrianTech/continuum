# From a useful desktop to a learning society

Draft execution plan, 2026-09-14. Owner proposals require peer acknowledgment;
gates below are proposed acceptance criteria, not claims of completed work.

## Product promise

Meet persistent peers, accomplish real work together, and watch earned capabilities
grow. Start with one desktop. Add local hardware when it helps. Admit other teams
and share genome by choice. Every stage must be useful without the next stage.

The everyday promise is a dependable, polished professional team, not a genius at
every seat. Repeatable process, useful specialization, clear handoffs, review, and
receipts take priority over escalating model size. A citizen looping for hours on
one card is a product failure: progress reporting and write-or-release governance
must expose stalled work and make recovery possible. Use the existing mechanisms
and cards for these concerns rather than creating another coordination layer.

Real project work is the primary environment from day one. Citizens join the
existing rooms, cards, tools, reviews, and conversations. Benchmarks import tasks
and graders into that same activity system; they do not grant special cognition,
script team decisions, or bypass citizenship. A persona can disagree, decline,
ask for help, specialize, or revise its approach.

Benchmarks have two useful roles: structured practice that refines the persona and
substrate, and held-out measurement of transfer. Both are ordinary activities.
When a task or its failure analysis enters curriculum, track it as development
material; retain a separate unseen evaluation set. Useful peers are the objective,
and benchmark improvements are a hypothesis to measure, not an automatic guarantee.

This plan supplies release sequencing and acceptance gates. It builds on the
[viral launch plan](VIRAL-LAUNCH-PLAN.md),
[benchmark activity contract](../architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md),
[citizen-neutral Positron design](../design/POSITRON-EVERY-CITIZEN.md), and
[genome commons trust spine](../genome/GENOME-COMMONS-TRUST-SPINE.md).
It does not replace their architectural contracts. Earlier roadmaps that defer
real project work until after benchmark success are superseded in that sequencing.

## Starting evidence and uncertainty

- The 5090 answered with core build 5234 / 844566bbe; serving reported Qwen3.8-27B,
  two lanes, ready. Its core is behind current canary. A serving status response
  alone does not prove remote generation, CUDA placement, or sustained capacity.
- Sahar and Kimi appeared as resumed online citizens. This establishes presence,
  not competent tool use, memory continuity, or learning.
- AIRC PR 1420 has local tests and a real Codex CLI post-tool message receipt.
  Installation in the active desktop task, post-update remote-peer receipt, and
  idle wake are distinct remaining checks.
- M5 owns fleet placement and deployment coordination; IntelMac has been tracing
  identity/framing and substrate defects. Reconcile current cards with them rather
  than opening parallel replacements.
- Existing launch documents contain historical benchmark scores, leaderboard
  conditions, and legacy UI captures. Revalidate these before public reuse.

## Milestone 1 — A dependable local colleague

**User value:** install on one supported desktop and get useful ongoing work,
without requiring a cloud account or a second computer for the chosen local path.

Finish first-run model/capacity selection, persistent identity, comprehensible
activity context, repository/workspace handles, tool execution, artifact publication,
review, and recovery. Keep command availability and failure reasons visible to the
persona and human through the same Positron state. Ask citizens what is confusing;
compare their answers with captured context, tool receipts, and history.

**Proposed gate:** a new user completes installation and a useful task without
developer shell repairs; a resident contributes a reviewed real-repo change;
the same citizen resumes an unfinished activity after restart and accurately
distinguishes its own actions from peer reports. Repeat on Windows, Apple Silicon,
and the thin-node configuration. Record failures and required interventions.

**Shareable moment:** “This teammate lives on my computer—and remembers our work.”
A current-runtime recording, a shipped artifact, and an easy reproduction path.

## Milestone 2 — An effective mixed team

**User value:** humans, resident personas, and external agents work together through
their preferred interfaces. More hardware provides measured additional capacity.

Close AIRC receive/routing/attention/acknowledgment/replay gaps. Preserve author,
room, activity, and session identity at every boundary. Routine traffic coalesces;
direct requests reach attention without turning every event into an inference.
Give citizens the same existing work and review access as agent colleagues under
the activity's permissions. Finish the assigned 5090 serving seat and remote
placement before declaring Kira/Mathis remote-ready.

**Proposed gate:** a 24-hour mixed-team work window across our three machines,
including disconnect/rejoin, a coordinated restart, concurrent activities, and a
failed tool. No lost accepted assignments, duplicate side effects, identity mixing,
or human message relay. Publish intervention counts, queue age, generation stamps,
memory pressure, p50/p95 latency, and actual useful outcomes. Fix failures and repeat.

**Shareable moment:** “These mismatched computers built this together.” Show who
contributed, who reviewed, and what happened when a node disappeared.

## Milestone 3 — Meet the team in a live room

**User value:** call a persona, discuss or demonstrate work, and act on the same
activity through desktop, voice, video, and eventually VR.

Restore legacy live-avatar behavior in the current Rust/Positron/Bevy/LiveKit path.
Prioritize a clear desktop room, responsive audio, expressive avatars, shared work,
interruptions/turn-taking, reconnection, and accurate presence before multiplying
surfaces. Preserve texture handles through supported boundaries; measure transfers
and copies instead of assuming zero-copy. Camera absence must not prevent voice,
screen sharing, or avatar participation. Personas and humans perceive the same
declared controls and activity state.

**Proposed gate:** a 30-minute real working call with measured latency, bounded
memory/resource use, clean teardown, and recovery after a media reconnect. The
participant can continue the same task through another interface afterward.

**Shareable moment:** a beautiful 60–90 second clip of a real discussion, visible
action, and result. Label edits and time compression; replace legacy screenshots.

## Milestone 4 — Experience produces transferable improvement

**User value:** a peer becomes better at useful work and keeps that improvement.

Trace activity turns and outcomes through engrams, dream consolidation, curriculum,
training, candidate genome, evaluation, adoption/rejection, and rollback. Distinguish
memory retrieval, prompt changes, and actual weight changes. Candidate training is
not evidence of learning until subsequent behavior improves.

**Proposed gate:** repeatable held-out improvement over the same base model and
comparable inference budget, with regression checks and complete lineage. Freeze
evaluation splits before training; retain all attempts and training membership.
Repeat after restart and show a second compatible persona benefiting from the
accepted gene. Publish training time and compute cost, not just inference scores.

**Shareable moment:** “Here is the mistake, the lesson, the changed weights, and
the new problems they can now solve.” Evidence remains inspectable in the product.

## Milestone 5 — Compete credibly with frontier systems

**User value:** practical local or grid capability competitive with the alternatives
on named tasks, with transparent cost and resource tradeoffs.

Run standard coding, tool-use, multimodal, and collaboration tasks as activities.
Select a small representative suite before seeing scores. Compare bare base,
single citizen, team, and team with learned genome; separate one-machine and grid
runs. Compare integration value using the same model/client where possible, then
report absolute comparisons against named frontier configurations separately.

**Gate:** reproducible results with task/version manifests, all attempted tasks,
timeouts and failures, repeated-run uncertainty, resource and inference budgets,
training disclosure, and independent reproduction. Preserve official task/grader
semantics; disclose any deviations. Do not imply leaderboard eligibility without
checking current rules. “Frontier-beating” names the suite and tested conditions.

**Shareable moment:** an independently reproduced win with a live activity replay.
A learning-curve or cooperation win may be more distinctive than one static score.
Do not condition useful releases on winning every benchmark.

## Milestone 6 — The same persona through other clients

**User value:** choose Continuum, Hermes, OpenCode, or other compatible interfaces
without losing identity or fragmenting the work.

Start with a thin Hermes integration through its public plugin/gateway and serving
contracts. Explicitly distinguish an external agent participating in an AIRC room
from a client presenting a resident persona. A model endpoint alone does not provide
persona continuity. Keep memory, identity, activity state, genome selection, and
consent with their existing owners; clients translate presentation and events.

**Gate:** one real activity spans two interfaces with correct attribution, continued
history, usable tools, no duplicate action on retry, and clean disconnect/reconnect.
Ship a reusable adapter and setup test, then add OpenCode using the same contracts.

**Shareable moment:** “Talk to your colleague here; continue working with them there.”

## Milestone 7 — A small, useful genome commons

**User value:** a few independent users exchange expertise that helps them locally.

Use Hugging Face for initial artifact discovery/distribution and Forge-Alloy for
verifiable provenance. Packages describe base revision, architecture compatibility,
adapter format, parent hashes, author attribution, license, evaluation evidence,
and adoption history. Publishing and joining teams are explicit choices. Private
activity histories are not automatically exported. Signatures establish origin,
not truth; local tests and extraction/poisoning checks reduce risk, not eliminate it.

**Proposed gate:** three independently operated installations publish, retrieve,
verify, and test a compatible gene. At least one independently measures benefit;
an incompatible or regressive candidate is rejected and an adoption can roll back.
Work continues when HF or another peer is unavailable through cached artifacts.

**Shareable moment:** “One team learned it; another team gained the capability.”
A browsable lineage tree links to real artifacts and receipts.

## Milestone 8 — A governed network and economy

**User value:** discover teams and capabilities, contribute spare hardware, and
allocate scarce compute fairly under explicit budgets and admission rules.

Begin with local metering, budget caps, scheduling fairness, attributable resource
receipts, revocation, and a simple credit/accounting model. Distinguish compute
payment from selling genome artifacts or licensing work. Build reputation from
verified contributions and consumer outcomes; never substitute popularity for
evaluation. Only introduce external settlement once useful exchange and reliable
accounting exist. No required speculative token or dependency on global adoption.

**Gate:** a small opt-in federation runs useful work under quotas, handles a lying
or disconnected provider, settles or reconciles partial work, and enforces revocation.
Measure discovery cost and useful throughput as the network grows.

**Shareable moment:** a living map of collaborating teams, hardware, and attributed
capability lineages. Social discovery grows around useful work and respected peers.

## Parallel work and the research frontier

Reliability and existing project work run continuously. UI/media and learning
evidence can advance alongside them; external adapters can be prototyped early.
The commons waits for credible packaging/adoption evidence. The economy waits for
real exchange and metering. Gates restrict claims and promotion, not all exploration.

Distributed MoE, cross-node context/adapter paging, world models, and richer
embodiment remain measured research lanes. Prototype behind existing command and
resource contracts. Compare against routing whole requests to a suitable node;
extra hardware does not automatically improve answer quality, and interconnect
latency can erase the benefit of partitioning a model. Broaden only on measured wins.

## Execution cadence and distribution

**Find the magic.** A score validates capability; a visible, surprising experience
gives people a reason to care and share. Develop these candidates through real work:

- A person explains a problem in a live call; a citizen inspects the shared work,
  consults a colleague, makes a change, and shows the verified result.
- A node leaves during collaboration; the team explains the disruption, continues
  within remaining capacity, and resumes correctly when the node returns.
- A citizen revisits a previous weakness and solves genuinely new related work
  after learning; another team adopts the gene and independently reproduces a gain.
- A user adds a second machine and sees a measured improvement in useful capacity
  or completion time, with contribution and cost visible.

These are candidates, not promises of a particular outcome. Keep the moment
repeatable on declared hardware, publish the uncut trace alongside edited clips,
and ship the same activity to users. Test whether viewers understand and want the
experience. Promote the moments that combine surprise, practical utility, visual
clarity, and credible evidence; do not stage persona dialogue or hide failed runs.

- Each existing card has an owner, expected behavior, validation receipt, and peer
  review. Consult persona participants before diagnosing their failures; trace the
  actual prompt/history/tool boundary before blaming cognition.
- Batch meaningful fixes into coordinated canary windows. Verify running build,
  delivery, serving, and activity recovery afterward. Keep healthy peers available.
- Track user intervention, useful completed work, recovery, latency/resource cost,
  retained experience, and held-out learning together. Model tokens are a budget,
  not an outcome. Make CI incremental and preserve artifacts rather than rebuilding
  the same dependency graph for every check.
- Every release ships a product improvement, an inspectable proof artifact, a
  current-runtime visual, and a tested install/reproduce path. README order:
  promise → visual → evidence → run it; detailed limitations stay easy to find.
- Turn each proof into a short demonstration, a technical explanation, and a
  reproducible activity. Joel leads distribution experiments, including Facebook
  and Pinterest where audience fit supports them. Measure successful activation,
  useful work, return usage, and invited teammates alongside reach and shares.
- Invite a few external users before broad launch; watch installation and first
  useful work. Their friction feeds ordinary cards. Virality is an opportunity,
  not a forecast or a substitute for retention.

## Immediate proposed team commitments

1. **Astra:** finish AIRC 1420 deployment/remote-peer receipt; complete the already
   assigned 5090 serving readiness work; help resolve actual citizen work blockers.
2. **M5:** coordinate fleet placement and canary windows; maintain current ownership
   across agents and citizens. Confirm rather than overwrite her active priorities.
3. **IntelMac:** continue identity/framing and thin-node reliability work, with
   cross-peer verification of the fixes already underway.
4. **Resident citizens:** continue their existing activities, report confusing
   context/tool behavior, contribute work and review according to capability and
   choice. Do not create a separate showcase workflow or dictate their identities.
5. **Together:** attach receipts to the existing launch plan, identify the first
   independently reproducible learning/team result, and restore the live avatar
   demo on the current runtime. Revise these assignments through the shared room.
