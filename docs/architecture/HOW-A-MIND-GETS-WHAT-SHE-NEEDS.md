# How a mind gets what she needs: requirements for adaptive focus and allocation

Status: requirements for review, 2026-10-30. No code until this is agreed.

Joel, 2026-10-30: "Let's spend zero time on code, just all the requirements for what she does along with
others. Hypothetical scenarios. How does one get what one needs at the time, and how does the RTOS-like
intelligence (not some hard-coded disaster) organically fit these demands through many acts, like stoplights
or any other well-known adaptive system."

This document builds on what is already decided and does not repeat it:
- [FOCUS-CONTRACT](FOCUS-CONTRACT.md): one allocation, claims with levels, no permanent percentages, a
  learnable attention policy;
- [FOCUS-AS-ATTENTION-TEMPERATURE](../cognition/FOCUS-AS-ATTENTION-TEMPERATURE.md): focus is a target and an
  intensity, and every threshold becomes a policy function;
- [PERSONA-CONTEXT-PAGING](PERSONA-CONTEXT-PAGING.md): signals, not constants; paging is the primitive, and
  limits emerge as costs;
- [ONE-BUDGET](ONE-BUDGET.md): one authority over every resource, and grants that shrink by request;
- [MIND-CONTEXT-AND-ENGINE-CONFIG](MIND-CONTEXT-AND-ENGINE-CONFIG.md): the codec, levels, epochs and causality.

- the grid governance, which this plugs into and does not parallel:
  [GRID-MARKET-CLEARING](GRID-MARKET-CLEARING.md) (nested prices from the pager to the N-node mesh),
  [ADAPTIVE-GRID-INTELLIGENCE](ADAPTIVE-GRID-INTELLIGENCE.md) (placement by measured candidate plans, and
  learning the policy), [CAPACITY-FABRIC-AND-GOVERNOR](CAPACITY-FABRIC-AND-GOVERNOR.md) (capacity is live,
  held loosely), [BEING-SOCIETY-GOVERNOR](BEING-SOCIETY-GOVERNOR.md) (the governor serves time, never dictates
  work), [CONCURRENT-MIND-AND-GOVERNOR](CONCURRENT-MIND-AND-GOVERNOR.md) (govern resources, never gate the mind).

**Where this sits.** Joel, 2026-10-30: "it's a lot like the grid governance and in fact it will need to plug
into that, because with many nodes we should ideally be smart about where to put what and then how to adapt
within: slots, learning, so much. And context window RAG budgeting is merely part of a persona." The
mechanism already agreed is the nested market of GRID-MARKET-CLEARING: each tier publishes a measured price
for its resource and each claim below it is granted while its measured marginal value exceeds that price.
Placement across nodes comes first (ADAPTIVE-GRID-INTELLIGENCE's candidate plans); adaptation inside a node
follows. This document is the **persona end** of that market: the tiers below a lane (a mind, her
activities, her claims), of which her context window is one claim among several, plus the dynamics and the
scenarios that must pass from the grid down to her answer.

## 0. Why: every failure today was an open-loop rule

On 2026-10-30, on the M5 (four residents):

| Rule | Written as | What it did |
|---|---|---|
| Reply cap | 12,288 tokens, priority above her context | took her view's room; her view turned over every act; 75 reads to 2 edits an hour |
| Its replacement | her p90 output × 5/4 | 963 tokens; Aris cut three times in a row inside her thinking, with ~30k of her slot free |
| Act think budget | 1,024 tokens | larger than that grant in 12 of 17 acts, so it never stopped a think |
| Working window | 49,152 tokens | one machine's replay on one day, applied to every machine |
| Epoch start | a quarter, then half, of the window | a fraction someone picked |
| Slots | four fixed private windows of 60,672 (`kv_unified = false`) | an idle mind's room cannot serve a mind writing a large edit |

Each rule decided without observing the effect of its own decision. **An open-loop rule is the defect
class.** Swapping one number for another, a constant for a fraction or a fraction for a percentile, is the
same defect. The requirement is closed loops: measure, decide, observe the effect, adjust.

## 1. What she does, and who else is there

She is one mind holding several activities at once, one of them in focus (README; the event mind). Each
activity has its own demand shape:

| Activity | Needs whole | Can fold | Output | Latency |
|---|---|---|---|---|
| Coding | the file she is changing, the newest result (build, test, error), the task | related files to outlines, older results to outcomes | heavy-tailed: a one-line fix or a whole-file write | her own pace; a person may be waiting on the PR |
| Conversation with a person | the person's latest messages and the thread's open questions | older turns to the room's history | short to medium | a person is waiting, every second shows |
| Review | the diff and the claim being reviewed | the repo around it | medium | a peer is blocked on the verdict |
| Chess or another game | the position, which is a sufficient statistic of every earlier move | all earlier moves | tiny | per move |
| Writing a book | the outline and character bible, the current chapter | earlier chapters to summaries | long | none |
| Live call, voice, vision | the current frames and the last utterances | everything older | streamed | tight and continuous |
| Dreaming, learning | her own engrams | everything | not to a room | none; it takes slack only |

**Others sharing the machine:** other residents on the same engine; minds leased in from other nodes; the
dream stage; residents' own builds and tests (CPU and memory); the person's own apps (a game, Photoshop, a
browser); the OS.

## 2. Resources and how capacity is known

| Resource | Unit | Capacity comes from | Consumed by |
|---|---|---|---|
| Device memory for KV state | tokens of state | free memory measured each tick ÷ the engine's measured bytes per token | every mind's held context and output |
| RAM page tier | bytes | measured free RAM, minus what the person's apps use | paged-out minds, cached results |
| Disk page tier | bytes | measured free disk, by cache class (CLAUDE.md eviction rule) | spilled minds, spilled results |
| Prefill throughput | tokens per second | measured per model on this node, under the current contention | every act's fresh tokens |
| Decode throughput | tokens per second | measured per model on this node, by how many generate at once (the decode knee) | every act's output |
| CPU | cores and seconds | measured load | tools, builds, tests, the core |
| A person's patience | seconds | the person is waiting or not (an event, not a guess) | a conversation's reply |
| Network | bytes and seconds | measured per peer | minds leased across the grid |

A capacity is a measurement with a time. **Unknown is unknown, never a default.** A decision made while a
capacity is unknown says so, and the loop measures before it trusts.

## 2a. The contention: many minds, many activities, one machine

Joel: "This has to contest with lanes, total available memory to us, many personas all needing some action
and focus, per their own activity they're focusing on, through a variety of activities like coding, talking,
live chats."

Two resources are contended, and every demand is on both:
- **Space:** memory for each mind's held state, from the node's measured free memory down to her answer.
- **Time:** lanes, the generations the engine runs at once, and turns on them. How many lanes is the
  measured decode knee (more lanes at once make each slower; past the knee, total throughput falls), never a
  fixed count.

Every claim sits in one hierarchy, and one allocator solves all of it together:

```
grid: nodes and their offers (GRID tier: price plus transport cost, by gossip)
  node: measured free memory after the person's apps and the OS; CPU; disk
    engine: lanes (time, the measured knee) and state memory (bytes, measured per token)
      mind (her temperature: how active she is, aged so she is never starved)
        activity (coding, a person's conversation, a live call, a review, a game; one in focus)
          her claims: context levels, think, answer, genes resident, tool CPU and disk, dreaming
```

Prices flow down, bids flow up (GRID-MARKET-CLEARING section 2): each tier sees only the price above it
and quotes one below it, so no component needs the whole picture and the same algorithm holds on one node
or many. The value unit at the top is the activity's measured outcome; every price below it is derived.

**Space and time trade against each other**, which is why one allocator must own both:
- A mind without a lane now does not need her state on the device now: it can page to RAM and be
  prefetched before her next turn. Space follows time.
- A mind whose state is on the device starts her next turn without a re-prefill. Time follows space.
- A mind writing a large edit needs both: a lane for the whole write, and room for every token she writes.
- A live call needs a lane often and briefly, with little space; a long refactor needs a lane for a long
  time and much space. The traffic-signal analogy is exact here: phases (who generates) and green times
  (how long) are re-split each cycle from measured demand, with minimum greens (floors) and calls
  (a person's message, a live call's frame).

**A persona's claims, of which her context is one:**

| Claim | Resource | Notes |
|---|---|---|
| Her context, by level per item | state memory, prefill time | this is where the RAG budget lives; one claim among these |
| Her output (think, answer) | state memory as written, decode time on a lane | written tokens are the only ones that cost |
| Her genes (adapters resident) | device memory, page-in time | paged by what her mind is on (genome paging) |
| Her hands (builds, tests, tools) | CPU, disk, RAM | a resident's own build is hers to run (residents build freely) |
| Her senses and voice (vision, hearing, speech) | encode and decode time, small memory | critical faculties are gates, never priced (GRID-MARKET-CLEARING invariant 1) |
| Her dreaming and learning | slack only | the idle class; it yields within a step |
| Her memory (recall, embeddings) | CPU, RAM index | |
| Placement (which node serves her) | network, a node's whole offer | decided first, at the grid tier |

**Per mind, per activity:** each mind's demand is the sum of her activities' claims, weighted by which one
is in focus. The activity in focus gets detail and action; the others keep a floor (their chain heads,
their open commitments) and fold. A mind switching focus changes her claims, not her identity or her
state; a switch costs measured time, so it is priced (R7).

**What every scenario must show:** every mind that has work gets action within a time bound set by her
temperature and aging; every activity in focus gets the detail its acts measurably reference; the person's
apps and any live call get their floors first; nothing is clipped.

## 3. Requirements

Each is testable, and the scenarios in section 6 test them.

- **R1. No open-loop number.** Every bound in the system is the current output of a loop that measured the
  effect of its previous decision. The only inputs set from outside are physical measurements and human
  intent (what matters to the person now, such as "this conversation is live"). A reviewer refuses any
  number not traceable to one of those.
- **R2. Demand is revealed by her acts, not predicted by a rule.** What she reads, cites, edits, re-reads,
  expands and writes is her demand. Her working set is the items she referenced over her recent acts
  (Denning's working set). How many recent acts count is itself chosen by the loop: the span past which
  adding acts stops adding items.
- **R3. Focus.** Detail goes by causal distance from her current act and by which activity is in focus.
  Everything else folds to a cheaper level that keeps a path back. Nothing is dropped silently, chain heads
  stay whole, and her newest result stays whole (FOCUS-CONTRACT rule 4; MIND-CONTEXT causality).
- **R4. Flex, cleared by price.** Every consumer is a flex item: its basis is its measured current need, its
  minimum is its causal or required floor (a reservation, visible in the books, never traded), and its grow
  and shrink are marginal-value curves measured from outcomes. The container is measured capacity. A claim
  grows while its marginal value exceeds its tier's price; the price rises with measured excess demand and
  falls with slack. That is the whole rule: no share, fraction or percentile anywhere (GRID-MARKET-CLEARING;
  ONE-BUDGET).
- **R4a. Placement first, then adaptation within.** Where a mind, a model, a gene or a job runs is decided
  at the grid tier from measured candidate plans (ADAPTIVE-GRID-INTELLIGENCE); everything else in this
  document adapts within that placement, and a sustained mismatch is a signal back to placement.
- **R5. Closed loops at every timescale**, each with a measured error signal and a measured cost of
  switching (section 5).
- **R6. Both ways.** Any loop that can shrink a grant must also probe upward, or it ratchets down forever
  (the scheduled-upward-test rule).
- **R7. No starvation, no thrash, no relaunch.** Waiting raises priority (aging). A state changes only when
  the expected saving exceeds the measured cost of changing. The engine adapts live; it is never restarted
  to change shape.
- **R8. Every miss is a signal.**
  - A re-read of a folded item is a page fault, identified by causal identity (same action, same outcome).
  - A cut is a loss.
  - A refusal is an overflow.
  - Waiting on a page-in is latency.

  Each is counted per mind and per activity, and feeds the loop that caused it.
- **R9. The person's machine comes first.** Load from the person's own apps is capacity removed. The
  system gives memory back within a step and takes it again only by probing after the load leaves.
- **R10. Any hardware, any number of minds, one algorithm.** Only the measurements differ. When even one
  mind's working set cannot fit, the system says so loudly (an under-provision) and schedules or places
  her elsewhere. It never clips silently.
- **R11. Every decision has a receipt:** its inputs, the decision, the outcome. It is replayable on the dyno
  (FOCUS-CONTRACT rule 8).
- **R12. Her agency.** She can hold focus, release it, and ask for detail (expand). The system honors each
  request and prices it into the same allocation; nothing external silos her.

## 4. Adaptive systems that already solve this, and what each contributes

| System | Its loop | Maps to | What we take |
|---|---|---|---|
| **Adaptive traffic signals** (SCATS, SCOOT) | detectors measure flow and saturation every cycle; green splits, cycle length and offsets move in small steps toward less total delay; minimum greens for safety; a pedestrian button is a call; a corridor is coordinated into a green wave | the intersection is the engine; each approach is a mind; green time is engine turns and KV; saturation is a mind's pending work; minimum green is her floor; the pedestrian call is a person's message; the green wave is a warm prefix, so consecutive acts flow | re-split every cycle from measured saturation, in steps bounded by measured switch cost; the cycle length (how long one mind holds the engine) adapts to total demand; there is no fixed plan |
| **Page-fault-frequency working-set control** (operating systems) | a process faulting more than its target gets frames; one faulting less loses frames | her re-reads of folded items are her faults; her window and her view's levels are her frames | the target fault rate is not a constant: it is where a fault's measured cost (the seconds of a re-read act) equals holding's measured cost (the prefill seconds per act of the extra tokens) |
| **Congestion control** (TCP; BBR in particular) | grow until loss, back off on loss; BBR models the bottleneck's bandwidth and round-trip time from measurements and paces to them | a mind's grant and her output room; refusal and memory pressure are loss | probing upward is mandatory (R6); prefer the model-based form (estimate capacity, pace to it) over a multiplicative constant |
| **CPU schedulers** (multilevel feedback queue, completely fair scheduler, RTOS priority inheritance) | interactive work is boosted; long work is demoted; a periodic boost prevents starvation; virtual time keeps shares fair; a task holding what others wait on inherits their priority | a mind with a person waiting is interactive; a long refactor is batch; dreaming is the idle class; a mind whose review blocks a peer inherits the peer's priority | one temperature per mind plus aging (plan goal 9) |
| **Video rate control** (codec buffer models) | bits go to frames by measured complexity; a buffer smooths; keyframes reset prediction | tokens go to items by how much they change; the epoch start is a keyframe; levels are mipmaps | spend detail where content changes; choose the keyframe interval from measured turnover cost |
| **Foveated rendering and human attention** | full resolution where the eye looks, coarse at the periphery; eye tracking measures the gaze; a saccade moves it | her fixation is her current act's target; folds are the periphery; a focus switch is a saccade | detail follows where she measurably looks (her references), not only declared importance |
| **DVFS governors, garbage-collector pacing** | state follows measured utilization; transitions cost, so they are paced | engine geometry, residency, epoch length | change state only when the measured gain exceeds the measured transition cost |
| **Flexbox** | basis, grow, shrink and minimum content; the layout reflows when the container changes | every claim, in every container from the node down to her answer | the shape of the allocation interface (R4) |

## 5. The loops, by timescale

Each loop runs on events, not timers, and each is slower than the loop inside it (cascade control), so they
do not fight. Each is one tier of the nested market: it measures its tier's excess demand, moves its price,
and the claims below it re-clear against the new price.

| Loop | Runs when | Measures | Decides | Stays stable by |
|---|---|---|---|---|
| L1 Token | while she generates | tokens written; pool free; the lane time she holds and who waits on it | her output keeps growing while its marginal value exceeds the price of the space and the lane time it takes; when the pool is tight, it asks colder state to page out, or she finishes | the ceiling is the pool and the lane price, both measured, never a guess |
| L2 Act | at each prompt build | her last acts' faults, cuts, prompt cost, outcome | one allocation over her claims: view, tools, grounding, think, answer floor | grants change only between acts |
| L3 Epoch | when her appends reach her grant | her turnover cost (re-prefill at her prefill rate) against her miss cost (her fault rate × a re-read act's seconds) | the size of her next start | the start is fixed for its epoch (append-only within it) |
| L4 Residency | when any mind's temperature or the pool changes | each mind's temperature, page-in cost, when her pending task completes (task handles say when she wakes) | which minds' state stays on device, in RAM, on disk; prefetch before a wake | a page moves only when expected saving exceeds measured page cost |
| L5 Admission | when demand or the decode knee changes | sequence capacity (#5188), decode rate by concurrency | how many minds generate at once | the measured knee, with aging so no mind waits forever |
| L6 Node | when external load, heat or a training reservation changes | free memory, the person's apps, thermal state | the pool's size; returning memory to the person | gives back at once, takes back only by probing |
| L0 Placement | when a node joins or leaves, offers change, or a tier below signals a sustained mismatch | offers and prices by gossip; measured transfer cost; each node's outcomes | which node hosts each mind, model, gene and job (ADAPTIVE-GRID-INTELLIGENCE candidate plans) | hysteresis and explicit switching cost; a price flicker never re-homes a mind |
| L7 Policy | over hours and days | outcomes of controlled probes: R6's upward tests recorded as experiments, each a bounded, receipted change to one grant for one act class, compared with matched acts without it | the value curves L2 and L3 use for each activity | a new curve replaces the old only after beating it on replay (FOCUS-CONTRACT); never learned from passive correlation, since hard tasks both think longer and fail more (Cormac on #5607) |

## 6. Scenarios

Each scenario becomes a dyno replay first (captured acts, no live residents) and a live read second. Its
pass criteria are measured, never "looks right".

**S1. One mind fixes a bug.** She reads six files, finds the bug in one, writes a large edit, the build
fails, she reads the error, fixes it, and opens a PR.
- Focus narrows as she works: the file she edits and the newest build output stay whole; the other five
  fold to outlines with a path back; her tools narrow to the coding subtree (goal 10).
- Her edit grows token by token from the pool (L1); nothing cuts it while room exists anywhere in the pool.
- If she re-opens a folded file, that is a fault (L2): that file's level rises and stays raised while she
  keeps referencing it.
- Pass: no write cut while the pool had room; no fold she re-read more than once without its level rising;
  the PR opened.

**S2. Four minds code at once.** One writes a large refactor; two wait on builds; one reviews a peer's PR.
- The refactor's mind is hot: its output and view grow into what the waiting minds are not using.
- The waiting minds' state pages to RAM (L4), and is prefetched when their build tasks complete, so their
  next act starts warm.
- The reviewer inherits the priority of the mind waiting on her verdict (priority inheritance).
- Pass: writes per hour and median act time beat four fixed slots on the same captures; no mind starved; no
  page-in on a wake that the task handle predicted.

**S3. A person messages a mind mid-refactor.**
- The message is a call (the pedestrian button): her interactive priority rises (L5) and her focus shifts.
- The conversation's chain gets detail; the refactor folds to its chain head, whole at the head, with its
  state paged, not dropped.
- She answers within the person's patience, then returns: the refactor re-expands from the page, not from a
  re-prefill.
- Pass: reply latency to the person; refactor resumed with no re-read of what she held before.

**S4. The person starts a game, then quits.**
- The node loop (L6) sees memory removed on its next measurement and shrinks the pool by request.
- Coldest minds page out first; windows follow at their next epoch starts; the hot mind keeps her working
  set; if memory is still short, fewer minds generate at once, and none is killed.
- The game quits: the pool grows back by probing (R6), minds page back in by temperature.
- Pass: the game gets its memory within a step; no relaunch; no dropped turn; everything restored.

**S5. Chess.**
- The position is the sufficient statistic: every earlier move folds into it. Her context is tiny and her
  act is fast.
- A source declares its sufficient statistic; the codec does not need to know what chess is.
- Pass: context size flat over a whole game; latency per move flat.

**S6. Writing a book over days.**
- The outline and character bible stand at the head; the current chapter is whole; earlier chapters are
  summaries she can expand.
- An expansion is demand paging; a chapter she keeps expanding stays at a higher level (L2).
- Pass: consistency facts held across restarts; expansions per chapter fall as the loop learns.

**S7. She keeps re-reading one file.**
- Causal identity sees the same action with the same outcome.
- She is shown the repeat as a repeat ("you read this at #12; unchanged"), and the fault raises that file's
  residency rather than fetching it again.
- Pass: no file read more than twice unchanged within an epoch.

**S8. An unknown, smaller machine** (a 24 GB GPU; a 32 GB unified-memory laptop).
- The same loops run on smaller measurements: fewer minds generate at once, windows are smaller, more state
  pages.
- A mind still gets a working set that fits her current focus.
- If even one mind's working set cannot fit, the system says so (an under-provision) and places her
  elsewhere on the grid or schedules her; it never clips.
- Pass: the same scenario suite passes, slower, with no code or configuration change.

**S9. A brand-new mind, nothing measured.**
- She starts with what the container offers; every loop measures from her first act.
- Priors come from the population: other minds' measured shapes for the same activity. A prior is learned
  data, never a constant.
- Pass: her loops converge toward her own measurements within her first acts, and the receipts show it.

**S10. A mind leased from another node.**
- Her state lives where she is served; network time is a measured cost in the same allocation; the pool
  spans the nodes she can be placed on.
- Pass: the same receipts, with the node and network costs named.

**S11. Dreaming while minds work.**
- The dream stage is the idle class: it takes slack only and yields within a decode step.
- Pass: no act slower with dreaming on than off; dreaming makes progress whenever slack exists.

**S12. A huge tool output** (a 51,827-character file, as Aiko read on 2026-10-25).
- It enters whole if her focus and the pool allow; otherwise as an outline with a handle, and she expands
  the parts she needs.
- The newest result is never cut; if it cannot be whole, the capacity outcome is explicit (FOCUS-CONTRACT
  rule 4).
- Pass: no newest result cut; parts expanded on demand.

**S13. A mind thinking at length.**
- Thinking stops where more thinking stops paying: the measured relation, per mind and activity, between
  think length and whether the act changed the world, weighed against the seconds a longer think costs
  her and the minds waiting on the engine.
- Her answer always has its measured room after the think.
- Pass: no act cut inside thinking with room left in the pool; think length tracks its measured value.

**S14. A deploy or restart.**
- The engine is adopted; minds' state was paged, so they resume warm. The first acts are a restart, and
  are compared only with a restart-only control.
- Pass: no relaunch; reuse on resume close to the pre-restart level.

**S15. The whole mix at once.** One node: eight minds; the person is in a live voice call with one; two
are coding (one writing a large refactor); three are talking in rooms; one is reviewing a PR another is
waiting on; one is idle; and the person starts a game partway through.
- Lanes rotate by measured demand like signal phases: the live call is called often and served briefly;
  the refactor holds a lane for its whole write; the room talkers share what is left by temperature; the
  idle mind has no lane and no device state.
- Space follows time: minds waiting for a lane, or on a build, page to RAM and are prefetched before their
  turn; the call and the refactor keep theirs.
- The reviewer inherits the waiting mind's priority until her verdict is in.
- The game starts: the pool shrinks by request; the idle and coldest minds' state leaves the device first;
  the call's floor holds; the refactor keeps its working set; room talkers see longer waits, not clipped
  views.
- Pass: the call's latency within the person's patience throughout; the refactor's write never cut; every
  mind with work acted within her aged bound; the game got its memory within a step; no relaunch, no clip,
  no dropped turn.

## 7. Signals required

| Signal | Per | Owner |
|---|---|---|
| prompt tokens, cached tokens, fresh tokens, prefill seconds | act | the engine's response timing |
| output tokens by channel (think, answer), decode seconds | act | the engine's response timing |
| references: which items she read, cited, edited, expanded | act | her acts' causal records (#5611) |
| faults: a re-read of an item her view held FOLDED; feeds L2 (raise that item's level) | act | the view, from causal identity (#5611) |
| repeats: a re-read of an item held WHOLE with an unchanged outcome; perception renders it as a repeat, and it never grows the item (Cormac on #5607) | act | the view, from causal identity (#5611) |
| reversals: per loop, decisions undone within a window, from the receipts; a scenario fails on it, so cascade stability is measured, not promised | loop | the receipts (R11) |
| stops: landed, cut in answer, cut in think, refused | act | the emission classifier |
| outcome: a typed result kind reported by the tool that did it (edit applied with its diff hash, message sent with its event id, PR opened with its number, move made, tool error), never inferred from her text. It is the system's service metric for tuning allocation; it never grades her normal work and never reaches a rendering of her or a gate (the no-harness rule) | act | the producing tool, on the act's causal record |
| waits: page-ins, queue time, a person's waiting time | act | serving and the room |
| free memory, bytes per token, external load, thermal state | tick | the node authority |
| decode rate by concurrency; prefill rate | model, node | `decode_knee`, `prefill_rate` |

## 8. Never

- Her newest result cut.
- A causal chain broken; an effect shown without its cause.
- Something dropped silently.
- A mind starved.
- A loop that oscillates.
- The engine relaunched to change shape while serving.
- A number not traceable to a measurement or a person's intent.
- One reader sizing itself outside the allocation.

## 9. What exists, and what is missing

**Exists:**
- an allocator inside one mind's turn (the turn's flex plan);
- the codec's levels and epochs (`continuum-context`);
- causal types (#5611, in review);
- the prefill-rate and decode-knee measurements;
- per-mind output peaks, persisted;
- slot save and restore;
- the paging and focus designs listed at the top.

**Missing:**
- one pool across minds (today: four fixed private slots);
- live engine reconfigure and paging verbs (SERVING-ENGINE S1 to S3);
- fault counting by causal identity;
- loops L2 to L6 as closed loops (today each is an open-loop rule);
- the policy loop L7.

**What is live today and how it reaches L1** (Cormac on #5607). Merged #5610 sends her measured need as
`max_tokens` and accepts a cut on a write larger than any she has made; S1's "no write cut while the pool had
room" contradicts it. On the M5 the cost was not one cut but three identical ones (Aris, 2026-10-30, 963
tokens inside her thinking, because the fixed 1,024-token think budget exceeded the grant). Two steps reach L1:
1. Within today's fixed slot: her output is bounded by space (the room her prompt leaves in her slot) and by
   time (the price of the lane time she holds, L5's tier price). Decode seconds are a cost even inside her
   own slot: they hold a lane other minds wait on, and with four slots on one engine, every concurrent decode
   also slows the others' (the decode knee). Kimi's 100,704-token turn ran 8 to 10 minutes inside her own
   slot. She keeps writing while her marginal value exceeds that price; it falls to zero when nobody waits, so
   a lone mind is bounded only by her slot (Cormac on #5607). Until L5 publishes a lane price, step 1 is
   space-bounded only, and the lane time each generation holds is a measured signal, not yet a price.
2. Beyond her slot: the engine must hold one pool (unified state) and extend a running generation's grant at
   a step boundary, a verb that does not exist yet (SERVING-ENGINE S1 to S3; owner Fable, engine).

Until step 2, `delib.emission.cut_mid_answer` and the think-only cut probe are the measured gap, and S1's
no-cut criterion applies after step 2 lands.

**Open-loop numbers to remove** (each becomes a ratchet entry that only goes down):
- the epoch start's half of the window;
- `ACT_REASONING_BUDGET`, the deliberation think share, the cold reply prior, the p90 with headroom;
- `WORKING_WINDOW_TOKENS`, replaced by L3's measured decision (her turnover cost against her miss cost),
  never deleted bare: a bare slot-sized window raised mean prefill per act in the replay (ONE-BUDGET
  contract 7; #5612 holds and folds into L3);
- `ACT_LATENCY_BUDGET_SECS`, `PASS_LATENCY_BUDGET_SECS`, `DECODE_FLOOR_TPS`;
- the fixed slot count and fixed slot window;
- every other number the inventory finds.

## 10. Decisions from review

Answers from Cormac's review of #5607 at `f2eb889cb`. Joel's word overrides any of them.

1. **Human intent:** a person may set which activity matters now, and that a conversation is live. Both are
   typed events, never weights. Everything else is measured.
2. **The objective:** least expected seconds to a typed world-changing outcome (section 7's taxonomy),
   weighted by temperature and aged. Guard: a person-waiting call's latency is never traded for throughput;
   its floor comes first (S15).
3. **Thinking:** bounded by its measured value to the outcome, measured only by L7's controlled probes.
4. **The value unit:** GRID-MARKET-CLEARING's numeraire is the activity's measured outcome. For her normal
   work that is the typed outcome of section 7: an observation of what the act changed, never a score of
   her.
5. **Ownership:**
   - L0 and L4 to L6 (placement, pool, admission, residency, node): Codex;
   - L1 to L3 and the engine verbs: Fable;
   - the outcome signals (taxonomy, faults and repeats, stops, the reversal counter) and L7: Cormac;
   - network capacity for leased minds: airc's NETWORK-AUTHORITY (airc#1628), which L6 consumes rather than
     measuring its own.
6. **The dyno:** Cormac builds the scenario suite from captures as part of the step-2 replay validator, and
   lists the captures each scenario still needs. S7 comes from the M5 read loops of 2026-10-30.
