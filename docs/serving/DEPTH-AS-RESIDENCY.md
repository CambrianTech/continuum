# Depth-as-Residency — deep thought mode via on-demand tier paging

**Origin:** Joel, 2026-08-03, on the heels of the device-cache proof: *"it could
potentially intelligently shift into a deeper thought mode with the paging going
on, from the native ram over to flash paging, on demand."*

## The idea

Thought depth and expert residency are the SAME axis. The pager already serves
experts from a tier ladder (device cache → host RAM cache → NVMe/flash stream),
and the measured curves say two things that make depth a *dial*:

- The routine working set is tiny (V4-Flash: 100% hits at 6GB, saturation ~12GB,
  measured 2026-08-03) — System-1 turns never leave residency.
- The cold-expert cost curve is graceful, no cliff (A2 UMA sweep) — paging deeper
  degrades tok/s smoothly, it never walls.

So instead of ONE residency config chosen at spawn, the mind (or the depth
planner, #223) requests depth per task, and depth actuates the pager's tier
policy:

| Depth | Residency policy | Cost shape | When |
|---|---|---|---|
| Reflexive | resident-only; miss = route to next-best resident expert | fastest, bounded | chat, backchannel, routine turns |
| Standard | resident + host cache, page on miss | today's serve | normal work |
| Deep | full expert population eligible; prefetch from NVMe ahead of routing; precision-on-miss upgrades (#282) | slow, maximal quality | SWE-bench tasks, hard debugging, deliberate reasoning |

## Why this composes instead of being new machinery

- **#223 adaptive-compute router** gains its actuator: depth requests were
  abstract; the pager tier policy is what they actually SET.
- **#126 self-scaling intelligence** gets its organic form: the persona doesn't
  swap models, she pages deeper into the one she has — governor-arbitrated,
  priced in latency she experiences.
- **#273/#281 TierPolicy seam** is the enforcement point — depth is an input to
  the plan `{tier, residency, prefetch}` that already exists per expert.
- **#282 precision-on-miss** makes deep mode also a *precision* upgrade: a
  deliberate turn is worth fetching the higher-precision expert variant.
- **Reasoning channel**: deep mode naturally pairs with reasoning-heavy decoding
  (the #181/#2136 lane) — slow tokens are acceptable exactly when the model is
  thinking, not speaking.

## Governor contract

Depth is a REQUEST, never a seizure: the governor prices it (latency + bandwidth
lease) against co-resident lanes and can degrade it under pressure — same lease
doctrine as every consumer (#56). A deep-mode turn on the 5090 borrows from the
~20GB co-residency headroom; on a 64GB Mac it borrows NVMe bandwidth. Never
thrash: depth transitions carry hysteresis like every lane (sticky, no
per-token flapping).

## First slice (when picked up)

Thread ONE bit — `deep: bool` — from the deliberation faculty's turn intent
through the serving request to the pager's plan, mapping to
{resident-only, today, all-eligible+prefetch}. Measure the tok/s and quality
delta on the eval battery per mode. That instrumented pair (fast score vs deep
score on identical tasks) is also a README-grade demo: the machine visibly
choosing to think harder.

## Detection: the router IS the depth sensor (Joel, refinement)

The shift is not only an upfront request — it is DETECTED mid-task: *"from 27
tok/sec running for high speed, detect it needed more experts, and adapt — slow
to 5 as it deeply thinks. Best of both worlds as long as our system can
accommodate."*

The detector already exists in the serving loop: the router's expert-selection
pressure against the resident set.

- **Cruise (fast)**: resident-set hit rate ~100% (the measured V4-Flash regime) —
  stay reflexive, serve at platform speed (27+ t/s on big-RAM Macs, 3-6 on GPUs).
- **Downshift trigger**: routing repeatedly wants NON-resident experts (miss-rate
  rising over a window, or routing-entropy spike — the #229 surprise/OOD signal).
  That IS "this input needs more of the model": widen eligibility, start NVMe
  prefetch, accept the slower tokens. The persona is now visibly "thinking hard".
- **Upshift**: miss pressure decays below the low-water mark for a sustained
  window → shrink back to residency and resume cruise. Hysteresis on both edges
  ([[never-thrash-sticky-hysteresis-on-every-lane]]); the two thresholds are
  governor policy, not constants.

This makes depth ORGANIC: nobody labels the task; the model's own routing
behavior reveals when the resident subset stops being enough, and the pager
answers with depth instead of forcing next-best-resident substitution quality
loss. Speed becomes an honest signal for observers too — the console shows
tok/s dip exactly when the mind is reaching deeper (#284's SCADA face renders
the downshift live).
