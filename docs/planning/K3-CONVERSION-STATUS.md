# K3 GGUF Conversion — status + the precise remaining gaps

**Status:** 2026-07-29, BigMama. K3 weights downloaded + verified (96 shards, 1.5TB). The converter
("model adapter") + fork now handle a large fraction of K3; this pins the EXACT remaining gaps so the
finish is a clean, reference-guided task (not error-by-error probing). Fork branch
`feat/kimi-k3-attnres`.

## DONE (converter runs through attention + gets into MoE)
- **MXFP4 dequant** (`conversion/base.py`): `mxfp4-pack-quantized` compressed-tensors branch — E2M1,
  group 32, E8M0 scale. K3's routed experts + anything `.weight_packed`/`.weight_scale`.
- **AttnRes tensors** (gguf-py + C++): `self_attention_res_{norm,proj}` → `attn_res_*`,
  `mlp_res_{norm,proj}` → `ffn_res_*`; the ggml graph op is built + compile-verified (48B byte-identical
  when `attn_res_block_size=0`).
- **MLA output gate** (gguf-py): `self_attn.g_proj` → `ATTN_OUT_GATE` (`blk.{bid}.attn_gate`). All 14 of
  K3's `self_attn.*` tensors now map. SiTU op exists (d82d02963).
- Converter progress: passes all of layer 0 (dense) + layer 1 attention; **fails at layer-1 MoE.**

## REMAINING GAPS (in converter-error order — each bounded, reference-guided)

### 1. Routed experts — `experts.N.w1/w2/w3` (MXFP4) → stacked GGUF expert tensors
K3 names the 896 experts' three projections `w1/w2/w3` (each `.weight_packed`+`.weight_scale`), NOT the
48B's `gate/up/down_proj`. **VERIFY from `modeling_kimi_k3.py` MoE forward which of w1/w2/w3 is
gate/up/down** (common convention w1=gate, w3=up, w2=down — but confirm, a swap silently breaks it).
Then map + let the existing MoE stacker build `blk.N.ffn_{gate,up,down}_exps` (MXFP4 per-expert).

### 2. **NOVEL** fused routed-expert transform — `routed_expert_up_proj` / `routed_expert_down_proj` /
`routed_expert_norm` (single tensors, NOT per-expert)
The 48B has NO equivalent. This is a K3 architectural component applied to the routed-expert path.
**Requires: (a) study `modeling_kimi_k3.py` to learn the exact forward (where norm/up/down apply
relative to the expert sum), (b) new GGUF tensor slots + tensor-map entries, (c) NEW C++ graph code in
`kimi-linear.cpp`'s MoE branch to apply it.** This is the genuinely new piece — correctness-critical,
M5's llama-serving lane.

### 3. Router + shared experts — likely already mapped (48B parity)
`gate.weight`, `gate.e_score_correction_bias`, `shared_experts.{gate,up,down}_proj` — verify these
resolve (the 48B had shared experts + a router; they're in tensor_mapping). Probably free.

## PAIRED C++ SERVING-CORRECTNESS CHANGES (needed before K3 serves right; conversion alone isn't enough)
- **MLA output gate**: apply `attn_output *= sigmoid(g_proj(x))` before `o_proj` when the gate tensor is
  present (ref `modeling_kimi_linear.py:470-472`). 48B path untouched (tensor absent). NOT yet in C++.
- **AttnRes graph**: DONE (compile-verified).
- **Routed-expert fused transform** (gap 2's C++ half).
- **MXFP4 native serving** vs dequant-to-bf16-then-requant: GGUF supports `MOSTLY_MXFP4_MOE` (type 38)
  natively — the faithful path keeps experts MXFP4 (no 2TB bf16 roundtrip). The current bf16 dequant
  path is the correctness-first proof; MXFP4-passthrough is the efficiency follow-up.

## THE validation gate (unchanged)
None of this is proven until **K3 generates coherent text**. The MXFP4 nibble-order, the w1/w2/w3
mapping, the routed-expert transform, the MLA gate — each silently "converts" if wrong. Coherent
generation on the served GGUF is the only real proof. Then the [[k3-paging-diagnosis]] + adapter-path
work makes it fast.

## Lane split (per the M5 collab)
BigMama has taken the converter through attention + MXFP4 + AttnRes + the MLA-gate mapping. The MoE
finish (gaps 1–2, esp. the novel routed-expert transform + its C++ graph) is deep llama-internals =
M5's serving lane; BigMama owns the 5090 serving target + validation once it converts.
