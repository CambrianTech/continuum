# Field config: GLM-5.2 UD-Q4_K_M on 4×3090 — 7-8 tok/s @ 256K ctx (Reddit, 2026-07-27, via Joel)

Independent practitioner config proving `-ot` expert-offload serving on a ~744B MoE in the
DISCRETE regime (4×3090 + DDR + numactl). Far-end evidence toward the don't-build-Model-B
verdict. Fork used: github.com/F3zz1k/llama.cpp (only for `--slot-save-auto`/`--cache-ram`;
everything else is upstream — verified present in our continuum/sync-2026-07-27 tree).

## The four adoptions (task #232)
1. **-ot layering (ADOPT):** hot overrides FIRST, catch-all `exps=CPU` LAST — first-match-wins.
   Note the PER-PROJECTION granularity (`ffn_up.*` vs `ffn_.*`) and multi-GPU split — a free
   tier between layer-granular slice-1 and the (deferred) per-expert Model-B.
2. **`--fit off` when hand-placing** — upstream auto-fit can override our `-ot` (#2043 check).
3. **KV quant `-ctk q8_0 -ctv q8_0`** — ~2× KV capacity; how he fits 256K.
4. **`--slot-save-path` KV persistence** (+ ngram spec decode `--spec-type ngram-mod`,
   n 48–64 ≈ copy-from-context; he found MTP unhelpful). Slot-save = our #205 re-prefill killer.

## His exact llama.cpp command
```bash
CUDA_VISIBLE_DEVICES=0,1,2,3 numactl --cpunodebind=0 --interleave=all nice -n -20 llama-server \
--model GLM-5.2-UD-Q4_K_M-00001-of-00011.gguf -ctk q8_0 -ctv q8_0 \
--fit off --ctx-size 262144 -b 4096 -ub 4096 -fa on \
--threads 64 --host 0.0.0.0 --port 5000 \
-ot "blk\.(9|10)\.ffn_up.*=CUDA0" \
-ot "blk\.(11|12)\.ffn_up.*=CUDA1" \
-ot "blk\.(3)\.ffn_.*=CUDA0" \
-ot "blk\.(4)\.ffn_.*=CUDA1" \
-ot "blk\.(5|6)\.ffn_.*=CUDA2" \
-ot "blk\.(7|8)\.ffn_.*=CUDA3" \
-ot "exps=CPU" \
--jinja -np 1 -kvu \
--slot-save-path /var/cache/llama.cpp/glm-5.2 --slot-save-auto --cache-ram 131072 \
--min-p 0.01 --top-p 0.95 --temp 1.0 --top-k 20 --reasoning-budget -1 --verbose \
--reasoning on --reasoning-preserve \
--spec-type ngram-mod --spec-ngram-mod-n-min 48 --spec-ngram-mod-n-max 64
```

(He also runs Qwen3.6-35B-A3B fully-VRAM via vLLM W8A8 + MTP spec — noted, we are
llama.cpp-native; ask M5 for the vLLM command if ever needed.)

BigMama: threads/batch/numactl notes apply to the 5090 box for the GLM-5.2 serve; sampler
block is his taste, not doctrine.
