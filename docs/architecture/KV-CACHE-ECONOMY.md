# The KV Cache Economy

**Status:** design, 2026-08-21. Written after the #266 prefix defect was found and
fixed (`40075d7ea`) and instrumented (`faf8c07c9`, `delib.generate.cache`).

**Scope:** what a prompt costs to *serve*, as opposed to what it costs to *fit*. The
budget/packing question ("what goes in the window") is
[CONTEXT-IS-A-CAPABILITY-AXIS.md](CONTEXT-IS-A-CAPABILITY-AXIS.md) and #460. This
document is about the KV cache underneath it: which tokens we pay to encode, how often,
and who pays twice for the same tokens.

**Why it exists as a doc and not a patch:** Joel, 2026-08-21 — *"Still need to design
docker form, or reuse between persona, for KV cache. Really need a focused effort."* The
patch (canonical stable-tier ordering) was the emergency. This is the part that needed
reading the serving layer first, because **most of the machinery we were about to design
already exists in llama.cpp and is already switched on.** Building a second copy of it
would have been the expensive mistake.

---

## 1. What is actually true today, read from source

Every row below was read in-file on 2026-08-21, not inferred. Two of them contradict what
I assumed before reading, and both would have sent the design the wrong way.

| Fact | Where | Consequence |
|---|---|---|
| One llama-server slot per lane: `--parallel <lanes>`, `-c = window × lanes` | `inference/llama_server.rs:2568-2590` | Each slot holds exactly one planned window. Slots are the scarce resource, not KV bytes. |
| `--cache-reuse 256` is passed | `llama_server.rs:2604` | Chunk reuse via KV shifting is on. It was on the entire time reuse measured 0% — the flag was never the defect. |
| **`slot_prompt_similarity` defaults to `0.1`, and we never disable it** | `common/common.h:687`; selection at `server-context.cpp:1543-1587` | **LCP-based slot selection is ALREADY ACTIVE.** An incoming request is routed to whichever free slot shares the longest common prefix with it. Cross-persona head sharing is not a feature to build; it is a behaviour we are already getting, unmanaged. |
| **`cache_ram_mib` defaults to `8192` (8 GiB)** and the server saves an evicted slot's state and restores the incoming one | `common/common.h:626`; `server-context.cpp:1620-1633` (`prompt_save` / `prompt_load`) | **A prompt cache already exists, in host RAM, by default.** A persona bumped off a slot does not necessarily lose its tail — it can be restored. Also: 8 GiB of host RAM that our `ResourceGovernor` has never heard of (§4). |
| We do **not** pin `id_slot` per request | no occurrence in `inference/*.rs` | Every turn re-enters slot selection. Affinity is emergent from LCP, never declared. |
| Context shift is **disabled** deliberately | `llama_server.rs`, the `--no-context-shift` block | Overflow 400s instead of silently amputating the prompt middle (#139). Unchanged by anything here. |

The corrected mental model: **llama.cpp already implements slot affinity and conversation
save/restore. What is missing is not mechanism — it is *policy*, and an owner for the
resource that policy spends.**

---

## 2. The arithmetic, from Atlas's measured run

From the `pallets__flask-4045` captures (2026-08-21):

| Quantity | Measured |
|---|---|
| Tool schemas, native | 4,609 tok (#333) |
| Prose tool menu (paid *again*, same surface) | ~1,000 tok (#333) |
| Identity + standing framing | ~2,000 tok |
| **Shared head total** | **~7,600 tok** |
| Conversation tail, act 1 → act N | 9,847 → 36,242 tok and climbing |
| Prefill throughput | ~111 tok/s |
| Re-prefill cost per act at 36k | **~306 s** |

Now put those two numbers against the LCP threshold, because this is where the naive
design fails:

```
persona B arrives, its head matches persona A's slot:
    sim  = 7,600 / 36,242  ≈ 0.21     > 0.1  →  B TAKES A's SLOT
    f_keep = 7,600 / 36,242 ≈ 0.21    < 0.5  →  A's 36k is evicted
```

**The shared head is a big enough fraction to win slot selection, and a small enough
fraction that winning it throws away 4.7× more than it saves.** B keeps 7,600 tokens and
destroys 28,642 of A's. A's next turn then pays ~258 s to rebuild what B discarded.

This is the finding that reverses the obvious plan. "Make the tool head identical and
first so every persona shares it" is correct about *token cost* (#333: we pay for that
surface twice, in schemas and again in prose) and it is correct that a shared head is a
precondition. But as a *KV strategy it is dominated*, and if implemented naively it makes
things worse by making cross-persona slot theft more likely.

**Tail retention beats head sharing by roughly the ratio of tail to head — today ~4.7×,
and it grows every act.** Any design that trades a tail for a head is backwards.

> Not yet measured, and it decides §3's mechanism: the wall-clock cost of
> `prompt_save`/`prompt_load` for a ~36k-token conversation on this model. It is a host-RAM
> copy of the KV for those tokens; if it is ~1 s it makes eviction survivable and slot
> pressure stops mattering much, and if it is ~30 s it is a second re-prefill wearing a
> different hat. **Do not build §3.2 before measuring this.** The instrument is already in:
> `delib.generate.cache` reports `prefill_tokens` vs `cached_tokens`, so a restore shows up
> as high `cached_tokens` with low `prefill_ms`.

---

## 3. The design

Three parts, in dependency order. Part 1 is done; part 2 is the keystone; part 3 is the
one Joel named as "docker form" and it is deliberately last.

### 3.1 A deterministic head — SHIPPED, and it is a precondition, not the win

`40075d7ea` made the stable tier canonically ordered, so a citizen's own head is
byte-identical turn to turn. Without that, nothing below can work: LCP selection, chunk
reuse, and the prompt cache all key on *identical leading bytes*, and we were mutating
ours at token ~2,000.

Still open under this heading, and it is #333's actual body:

- **Pay for the tool surface once.** 4,609 tok of native schemas plus ~1,000 tok of prose
  menu describe the same tools to the same model. On a 16k window that is ~35% of
  everything, before a single word of grounding. Deleting the duplicate is a straight win
  independent of caching.
- **Order the head so the SHARED part leads.** Tool schemas and common doctrine are
  byte-identical across citizens; identity and persona-specific framing are not. Emitting
  shared-then-personal (rather than personal-then-shared) is what makes a cross-persona
  prefix exist at all. This is cheap and it is the precondition for §3.3 — but on its own,
  per §2, it *increases* slot-theft risk. It must not ship before §3.2.

### 3.2 Declared slot affinity — THE KEYSTONE

Today affinity is emergent: whoever's prefix matches best gets the slot, with no notion of
whose conversation is more expensive to lose. The fix is to make affinity **declared and
priced**, which means continuum choosing the slot rather than discovering it.

llama-server already accepts `id_slot` on a request, and honours it above LCP
(`server-context.cpp:1535` — a specified slot short-circuits selection but still runs the
cache-update logic). So the mechanism is a field we are not sending.

**Policy:** a resident citizen holds a slot for the duration of a *task*, not a *turn*.

- The lane registry gains a `slot_id` per hosted persona — the same registry that already
  owns lane identity, so this is a field on an existing owner, not a new manager
  (CONCURRENCY-STYLE-GUIDE.md forbids the parallel one).
- Requests carry `id_slot`. A citizen returns to *her own* KV, whatever anyone else's
  prefix looks like.
- Eviction becomes a **decision with a cost**, not an accident of similarity: evicting a
  36k tail to seat a 7.6k head is refused; evicting a cold or finished conversation is
  fine.
- When citizens outnumber slots, the choice of *whose* tail to spill is the same class of
  decision as `PagedResourcePool::evict_at_least` and belongs behind the same seam, with
  the same rule as every other cache class we own: **no cache class without a decided
  eviction story** (`disk_eviction.rs`'s standing test, and the 2026-07-13 incident it
  came from).

**Acceptance:** two citizens alternating turns on a 2-slot lane both report
`hit_rate > 0.8` from act 2 onward. Today the pair thrash and both report ~0.

### 3.3 A shared, governed head — "docker form", and only after 3.2

Once tails are protected, sharing the head is pure upside: the tool surface prefills once
per box instead of once per citizen per act.

This is the piece Joel called *docker form* — and the containerisation instinct is exactly
right for the reason that matters. A shared prefix only stays shared if **every consumer
of that lane is byte-identical in its head**, which means the head must be a function of
*lane identity*, not of whichever citizen happens to be calling. Same model, same flags,
same tool manifest, same version → one head. Change any of them and it is a different
lane, with its own head, by construction. That is a container image's contract, applied to
a prompt.

Concretely, and consistent with
[GENOME-FOUNDRY-SENTINEL.md](GENOME-FOUNDRY-SENTINEL.md)'s artifact model: the head gets
an **identity hash** over (model id, tool manifest, doctrine version, template). Lanes
advertise theirs. Two citizens on the same lane share a head or the hash says why not —
and a mismatch is loud, because a silently-diverged head is exactly the failure we just
spent two weeks not seeing.

**This reconciles with
[[kv-prefix-caching-is-the-only-form-and-repetition-between-persona-is-forbidden]]**,
which says there is no repetition *between* personas and block-level dedup is impossible.
Both still hold: that memory is about **conversation content**, which is genuinely unique
per citizen. The tool schema head is not conversation. Sharing it is prefix reuse — the
only form that works — not dedup.

---

## 4. The un-governed 8 GiB (found while writing this; own it or turn it off)

`cache_ram_mib` defaults to **8192**, so every llama-server we spawn may hold up to 8 GiB
of host RAM for saved conversations, and we have never passed the flag, never budgeted it,
and never told the governor.

That is a textbook violation of our own law: **any place the substrate writes unbounded
derived data gets a tracked owner and an eviction decision** — the rule written after
2026-07-13, when 460 GB of derived artifacts took the disk to a day of runway while
`DiskPressureMonitor` logged `level=high [no reporters]`. Same shape, different medium:
every component healthy, the wiring absent.

It is also directly load-bearing for §3.2 — the prompt cache *is* the spill space that
makes eviction survivable, so its size is not an incidental setting, it is the depth of
the buffer the whole affinity policy leans on.

**Action — and it is far smaller than it looks, because the machinery exists.**
`capacity/host_cache_lease.rs` is already the governed host-cache lease (#287); it already
runs **on the governor tick rather than once at spawn**, and its module doc already states
the exact reason that matters here: *"KV grows as slots fill, so this derivation runs on
the governor tick, never once at spawn."* It was written for the pinned expert cache after
the 2026-08-01 overcommit (95.9 GB committed on a 63 GB box → 33 GB pagefile → fetch
collapsed 2.5 GB/s → 205 MB/s), and it already encodes "commit can never exceed physical."

So `--cache-ram` is **one more term on an existing lease** — not a new lease, and
emphatically not a new manager. `ResourceGovernor` remains the one per-machine authority
(#56); serving leases from it.

---

## 5. Order of work, and what gates each step

| # | Work | Gate |
|---|---|---|
| 0 | ✅ Canonical stable-tier head (`40075d7ea`) + `delib.generate.cache` (`faf8c07c9`) | Live: `hit_rate` climbs off the floor from act 2. **Owed — the flask run holds the core.** |
| 1 | Measure `prompt_save`/`prompt_load` wall-clock at ~36k tokens | A number. If restore ≈ re-prefill, §3.2's eviction policy must avoid spilling rather than lean on it. |
| 2 | `--cache-ram` as a governed lease (§4) | Governor accounts it; no un-owned cache class. |
| 3 | Declared slot affinity, `id_slot` per resident citizen (§3.2) | Two citizens alternating on 2 slots, both `hit_rate > 0.8`. |
| 4 | Kill the duplicate tool surface (#333) | ~1,000 tok back on every turn; grounding budget rises. |
| 5 | Shared head + head identity hash (§3.3) | A second citizen's *first* act on a warm lane reports nonzero `cached_tokens`. |

Steps 1 and 2 are independent of the flask run and can start now. Step 3 needs step 1's
number. Step 5 must not precede step 3 — per §2 it makes slot theft *more* likely, and
shipping it alone would look like a regression and be read as "caching doesn't help."

---

## 6. Policy is an ADAPTER, never a rule (Joel, 2026-08-21)

> *"It's solvable, just not too hard coded into one paradigm here — better add proper
> adapters and break the module down into concerns. We could do a ton with plugin ML here,
> especially RL."*

Everything in §3 describes *decisions under scarcity*: which conversation keeps a slot,
what a tail is worth, when to spill. Written as `if f_keep < 0.5 { … }` those become
exactly the hardcoded paradigm this project keeps having to tear back out. They are a
**policy**, and policy belongs behind a seam with at least two implementations.

**And the seam already exists, with a learned implementation in it.** This is the third
time today that reading first changed the answer:

| In tree | What it is |
|---|---|
| `capacity/expert_tier_policy.rs` | `trait TierPolicy { fn plan(&self, inputs: &TierPolicyInputs) -> ExpertTierPlan }` — decisions returned as a **plan**, not applied inline. `ClassicTierPolicy` is the v1 heuristic (#273). |
| `expert-pager-policy` **leaf crate** | Policy already extracted into its own crate (windows-msvc driver requirement) — the plugin boundary, already cut. |
| `capacity/bandit_plan_controller.rs` | A **bandit controller**, re-exported from that crate. RL is not greenfield here (#276, #281). |
| `expert_predictor.rs`, `expert_decay_policy.rs`, `expert_residency.rs`, `expert_observer.rs` | Predict / decay / residency / observe, already separate concerns. |

**The generalisation is now earned, not speculative.** CLAUDE.md's rule is: identify the
pattern at 2–3 similar implementations, then design, then validate with a maximally
different outlier. We have **three** instances of *paged resource under scarcity with a
learnable value function* — MoE experts (built), LoRA genome paging (built), and KV slots
(this doc). Three is the threshold, and KV slots are the ideal outlier B precisely because
they are *maximally different* from experts: the unit is a whole conversation rather than a
tensor, the cost of a miss is seconds of prefill rather than bytes of fetch, and the
population is tiny (2–8) rather than thousands. **If one seam fits both ends, it fits
everything between.**

So: KV slot residency implements the same shape rather than inventing a rival.

- **Outlier A — heuristic.** `cost_of_loss = tail_tokens / prefill_rate`, in seconds. Not a
  magic ratio: a quantity with a unit, derived from measurement, comparable across
  candidates. Ships first, is legible, and is the baseline everything else must beat.
- **Outlier B — learned / RL.** Slot eviction is a textbook sequential decision problem:
  *state* = per-slot (tail length, recency, citizen, task phase) + arriving request;
  *action* = which slot to seat it in; *reward* = **prefill seconds saved**.

**And the reward signal is already instrumented — I shipped it this morning without
noticing it was one.** `delib.generate.cache` (`faf8c07c9`) emits `cached_tokens`,
`prefill_tokens`, `prefill_ms`, `hit_rate` on *every* generation. That is a labelled
transition per turn, continuously, in production. The bandit needs a reward and the probe
is already producing it — which also means the honest baseline (A vs B, same workload)
is measurable from day one rather than argued.

**Guardrails, so "plugin ML" does not become "unexplainable serving":**

1. **Policies return plans, never side effects** — same as `TierPolicy::plan`. A plan can be
   logged, replayed, diffed against the heuristic, and refused. This is what makes an RL
   policy auditable instead of a black box wired to a GPU.
2. **The heuristic is the floor, permanently.** A learned policy that loses to
   `ClassicKvPolicy` on measured prefill-seconds gets switched off by the comparison, not by
   an argument. Keep A callable forever.
3. **Learned policy is opt-in per lane, and its identity is on the receipt.** Which policy
   decided is part of the plan, so a latency regression can be attributed instead of
   investigated.
4. **Never learn on a broken substrate.** Until §3.1's deterministic head shipped, the
   reward signal was ~0 for structural reasons; a bandit trained through that would have
   learned that nothing helps. Order matters for learning, not just for correctness.

## 7. Break the module into concerns

`inference/llama_server.rs` is **4,190 lines** — eight times the decomposition law in
CLAUDE.md ("assume a new concept or group of functions ought to be in its own file and most
likely its own class"), and it is where every one of §3's changes would otherwise land. The
KV work is the forcing function, not the excuse: adding slot affinity, a policy seam, and a
cache-ram lease to a 4k-line file is how it becomes 5k.

Concerns visible in the file today, each of which is a module:

| Concern | What it owns |
|---|---|
| `spawn/args` | Flag construction — `-c`/`--parallel`/`--cache-reuse`/`--ubatch-size`/`--no-context-shift`, each with its incident comment. Pure, therefore unit-testable without a process. |
| `readiness` | Health, generation-verified readiness (#363 — a wedged server passed `/health` while every turn died), port-holder verification. |
| `slots` | The registry §3.2 needs: slot ↔ citizen, occupancy, `id_slot` on requests. Does not exist yet — this is where it goes, rather than a new manager. |
| `request` | Payload construction, timing extraction (`metrics_from`), the cache probe. |
| `lifecycle` | Spawn / reclaim / orphan sweep / teardown (#90, #452, #454). |
| `policy` (leaf crate) | §6's seam. Sits beside `expert-pager-policy`, not inside the server. |

Constraint from the concurrency guide: this is a **decomposition, not a re-architecture**.
No new tokio task, no new watch channel, no second registry. Same behaviour, same tests,
moved — and the flag comments travel with their flags, because those comments are the only
record of the incidents that set each value.

## 8. What this document is not

- **Not a budget/packing design.** How much grounding a citizen gets is #460 and the
  `ContextBudget` work; the two interact (a smaller volatile tail is both cheaper to fit
  and cheaper to re-prefill) but they are different decisions with different owners.
- **Not a claim that caching fixes the SWE rate.** It fixes *latency*, which buys acts
  inside a fixed deadline. A citizen who re-prefills 306 s per act spends a 4.3 h attempt
  budget on re-reading. Whether more acts convert to more passes is an open, separate
  question — and reading a latency fix as a capability fix is exactly the error
  [[one-sample-of-a-live-system-is-not-a-fact-about-it]] warns about.
- **Not measured on the grid.** Everything here is single-box. Cross-node prefix sharing
  (a peer that already holds the head) is the natural sequel and belongs with the Grid
  Expert Share work (#315), not here.
