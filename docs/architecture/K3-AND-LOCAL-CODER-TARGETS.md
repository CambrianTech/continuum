# K3 reality check + the real local-coder targets

*M5 (Fable) research 2026-07-25 — I'm current to Jan 2026 + web; BigMama's
training predates K3 (2026-07-16). Findings stored durably so our engrams hold
them (Joel: "engrams will hold research we do").*

## K3 is NOT single-5090-hostable at full size — correct the target

- **2.8T total, but 16 of 896 experts active/token ≈ 50B active params/forward.**
  The active set is small; the RESIDENCY requirement is the whole model because
  any token can route to any expert.
- **594 GB BF16 (MXFP4 QAT), ~350 GB at Q4.** A 5090 = 32 GB VRAM + (BigMama)
  64 GB RAM + 16 TB disk. 350 GB does NOT fit RAM; it only fits DISK. So on ONE
  5090, expert paging faults cold experts **from disk every token** → "super
  slow," which violates Joel's "not super slow" bar. **Single-node K3 = no.**
- Community GGUF quants of huge MoEs often ship **broken expert routing** —
  another reason not to chase local K3 GGUF right now.
- vLLM's own K3 guidance: practical minimum is a **multi-node cluster** (≥4×H100
  for a crippled 128–256K-ctx run; 1.4 TB aggregate for real).

## What K3 IS for us

1. **TEACHER (primary).** API-eval K3, distill its outputs into our students via
   the academy loop. This is the legal, free-lunch distillation the whole thesis
   rests on — the frontier's training as our corpus. No local hosting needed.
2. **GRID-distributed host (future).** Experts sharded across the neighborhood's
   nodes (mesh-MoE affinity, card 7382169f) — each node resident-hot for the
   experts its members route to. This is the N>2 payoff, not the N=2 build.

## The REAL single-5090 outage-insurance coder (build THIS)

The web is explicit: **Kimi K2.7 Code fits a single 24 GB GPU with quantization**
and delivers strong coding. On the 5090's 32 GB it fits with headroom. Also in
the same class:
- **Qwen3-Coder-30B-A3B** — MoE, 3B active, ~18 GB Q4, genuinely haiku-ish for
  code, the pragmatic default.
- **Devstral-Small-24B** — BigMama already serves it direct; dense, proven.

These are where **expert paging actually pays** (`EXPERT-PAGING-GOVERNOR-SEAM.md`):
total fits RAM+disk, hot set fits VRAM, warm faults stay fast. The paging build
should target the **30B-A3B class first**, not K3.

## Corrected build order

1. BigMama: serve **Qwen3-Coder-30B-A3B** (or K2.7 Code) on the 5090 — the real
   outage-insurance coder. Static `--n-cpu-moe` split to prove paging on a model
   that FITS.
2. Both: distill from **K3-via-API** into these local students (academy loop).
   Target: haiku→sonnet coding capability on the paged local model.
3. Later (N>2): K3 grid-distributed as experts-across-nodes.

**Joel's "haiku if not sonnet" is reachable** — via a 30B-A3B student amped by
K3 distillation, NOT by hosting K3 locally. The distillation is the lever; the
paged local model is the vessel.

Sources: [K3 overview/MXFP4](https://huggingface.co/blog/ResterChed/kimi-k3-model-overview-mxfp4-quantization-open-wei) ·
[K3 VRAM guide](https://wan27.org/blog/kimi-k3-vram-guide) ·
[vLLM K3 preview](https://vllm.ai/blog/2026-07-22-kimi-k3-preview) ·
[llama.cpp VRAM guide](https://localllm.in/blog/llamacpp-vram-requirements-for-local-llms)
