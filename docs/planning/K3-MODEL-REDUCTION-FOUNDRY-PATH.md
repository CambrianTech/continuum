# K3 model reduction — the foundry path (the smarter half of the dial)

**Status:** strategy locked 2026-07-30 (Joel). SEQUENCED AFTER the cache/paging work
(which we need anyway) — this is the dramatic-speedup complement, not a replacement.

## The dial (two ends, both ours)
- **Cache / paging** (agents building now, tasks #23/#28): serve the FULL K3 (662GB) on
  misfit hardware via GPU expert paging. General, full-quality, needed regardless. But it
  fights RAM/PCIe every token.
- **Foundry reduction** (THIS doc): DRAMATICALLY REDUCE K3 to a tailored model that fits
  VRAM+RAM DIRECTLY — no paging, full GPU speed. Tailored to OUR needs (our working set,
  our activities/benchmarks). This is what made the 19B ([[moe-expert-paging-feasibility]]:
  we already proved the subset step in `tools/scripts/compaction` — Plasticity Compaction).
The two compose ([[K3-PAGING-DIAGNOSIS]] synthesis): page the real expert for rare-domain
correctness; serve the compacted-to-fit subset for hot-domain speed.

## The arsenal to mine (Joel's pointers — SURVEY these when the cache lands)
Do NOT reinvent — we've built most of this. Survey first:
- **The foundry / forge** — the JIT/compaction system ([[continuum-substrate-already-built]]
  foundry-as-JIT). The "crazy shit we did before."
- **legacy widget** — old widget code holds prior foundry/compaction experiments. LOOK HERE.
- **sentinel-ai** ([[sentinel-in-substrate]]) — PGO/background-learning; picks the hot
  expert subset from real activation (`ExpertActivationProfile`) — the §4.1.3.4 prune driver.
- **forge-alloy** ([[forge-alloy-contract-attestation-layer]]) — the alloy = the compaction
  artifact + attestation; `forge-alloy/python/forge_alloy/types.py` has the alloy types.
- **targeted experiential plasticity** — the Plasticity Compaction that produced the 19B:
  prune to the hot subset + train a compensation-LoRA on a held-out corpus that recovers
  the pruned experts' accuracy ([[benchmark-learning-flywheel]]: the LoRA is trained from
  the catalog's GRADED FAILURES — the being learns the adapter that makes its pruned K3
  match full K3 on the exact tasks it's measured on).
- **variable quant sizes** (task #29) — asymmetric/regional quant: hot working-set experts
  high-bit, cold tail low-bit, to shrink the resident footprint to fit.
- **unet stuff** — [Joel-named; SURVEY: likely a U-Net-style compression/architecture piece —
  locate what it is and whether it applies to expert compaction. Flag when found.]

## The recipe (K3 -> tailored fits-in-VRAM model)
1. sentinel-PGO from real activation over OUR activities -> the hot expert subset that
   covers our working set (the §4.1.3.4 falsifiable prune).
2. Prune K3 to that subset (all-experts-available demoted to a fits-VRAM core; rare experts
   still reachable via the cache/grid for correctness).
3. Variable/regional quant the core to fit VRAM+RAM directly.
4. Train the compensation-LoRA from graded failures on our benchmark activities so the
   pruned+quantized core matches full K3 on the tasks we measure.
5. Emit as a forge-alloy artifact (attested, reproducible) via the recipe-as-entity foundry.
Result: a tailored K3 that serves ENTIRELY in VRAM at full GPU speed for our activities —
the misfit-design win without the paging tax.

## Sequencing (Joel)
"Once we've tried the cache stuff you're doing (which we need anyway)." Cache FIRST (it's
the general substrate + the full-quality/rare-domain path). Foundry reduction SECOND (the
tailored dramatic speedup). They are complements on the dial, not either/or.
Complements [[kimi-k3-grid-strategy]] (Path B forge/distill dense student, Path C
sentinel-PGO expert-subset prune).
