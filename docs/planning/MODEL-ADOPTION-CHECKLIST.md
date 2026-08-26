# Model adoption checklist — how a new base model becomes a citizen's mind

**The thesis this rests on** (frozen-borrowed-weights-as-ImageNet-backbone): adopting a new
base model is a **swap in `model_registry/catalog.rs`**, not a rebuild. The cognition /
genome / memory / tool layer is unchanged; only the backbone changes. The substrate work
of 2026-08-25 (residency, honest tools, live captures) is what makes this a swap. So the
whole question is: **can we SERVE it, and does it WIN?**

## The 5 gates (in order — a NO at any gate stops adoption, honestly)

1. **Format / architecture support.** llama.cpp on Metal is the serving floor. A model with
   a *redesigned* architecture (new attention/residual/embedding) is NOT day-1 GGUF-servable.
   - Check: does upstream llama.cpp support the arch yet? If not → our fork+converter path
     (the one that landed Kimi-Linear on Metal). "Consume and adapt to GGUF or whatever we
     need" is a first-class capability, not a one-off.
   - Output: a servable GGUF (some quant tier), or a dated blocker naming what's missing.

2. **Serveability on the target box** (M5, 64 GB today).
   - Weights at the chosen quant must fit resident, OR the pageable parts (n-gram embeddings,
     cold experts) must map through `PagedResourcePool` + the governor, with hot MoE experts
     resident. Compute the budget: `weights_at_quant + kv_at(ctx) + spike ≤ usable_gb`
     (`serving_plan.rs` already does this — read the `serving/plan` verdict).
   - If it only fits at Q2/Q3 with quality loss, consider a **compacted forge** (the
     qwen3-coder 19B-compacted precedent) before giving up.
   - If it can't fit on one box → it's a **grid signal** (n>2, split across machines), not a
     single-box adoption.

3. **Catalog registration.** Add a `ModelSpec` to `model_registry/catalog.rs` (mirror an
   existing entry): `gguf_repo` + `gguf_file`, context window, `kv_per_token`, capability
   rank, and the `ModelServingPrefs` (`mmproj_on_main_lane`, `kv_shiftable` — measure both,
   never assume; Ornith taught us that). `models/pull` fetches it into the shared cache.

4. **Head-to-head, same gyms.** Serve it, then run it against the **incumbent** (Ornith) on
   the SAME sets — never a fresh-vs-fresh claim:
   - Coding: `swe-bench-verified` (via `benchmark/dispatch`, see `benchmarks/swe/RUNBOOK.md`)
     + the rust gym ladder (`kick.sh`).
   - Cowork: the collaborative-room gyms (the emergent-alignment / team benchmarks) — this is
     where a "coding + cowork" model earns its adoption, and where our thesis is differentiated.
   - Report cost/energy per solve too (the four axes: score, cost, speed, learning).

5. **Adopt iff it WINS and SERVES.** A better score that only serves at unusable speed is not
   an adoption; a servable model that loses to Ornith is not either. Both, or it stays a
   candidate with a dated receipt.

## Immediate application: Qwen3.8-Flash-Next (releases ~2026-08-26)

125B total / 51B n-gram embeddings / **6B active per token**, multimodal MoE, "more capable
in coding and cowork," ~1/9 training cost vs Qwen3.7-Plus.

- **Gate 1 (arch):** redesigned attention/residual/embedding → assume NOT day-1 llama.cpp.
  Watch for a GGUF from Qwen/unsloth; if none, scope the fork-converter effort. **This is the
  gating uncertainty.**
- **Gate 2 (serve):** 125B @ Q4 ≈ ~63 GB weights alone — *over* 64 GB before KV. The 51B
  n-gram embeddings are the escape hatch IF pageable/mmap-able (likely) — resident hot experts
  + paged embeddings. Borderline; probably needs Q3 or a compacted variant. **Measure, don't
  guess.**
- **Gate 4 (win):** the "cowork" strength is the reason to want it — it targets our
  differentiated axis directly. Head-to-head on SWE + the cowork gyms.
- **Verdict shape:** if it serves at Ornith-class speed and beats it on coding+cowork, it's
  the new persona base and a real stride toward "Fable-class local by end of 2026." If it
  can't fit on one box, it's the first strong case for the grid split.

## The teammate-readiness finding (2026-08-25 audit) — separate from the model

**Our citizen tools are KOSHER, not benchmark-fit.** The 26 `code/*` verbs (read, edit, write,
run, shell, search, glob, tree, diff, undo, cargo/check, cargo/test, git/add, git/commit,
git/push, git/diff, git/log, git/status, git/apply) + `web/*` + `tool/output` are **general
developer tools — zero benchmark-specific verbs.** Because benchmarks are adapters feeding
tasks into these general tools (never a parallel runner with special tools), the benchmark
work *generalizes*: she is already a real code-execution teammate.

**The gap that separates "code executor" from "friendly teammate":** she has LOCAL git
(commit/push/diff) but **no GitHub collaboration workflow** — no open-PR, review-PR,
comment, request-review, or issue-management verbs. "Friendly in how code and GitHub work are
managed" (Joel) needs that collaboration layer. **This is the teammate frontier**, and it's a
tools gap independent of which model we serve — a candidate for the takeover fix-list
([[citizen-takeover-substrate-must-be-self-legible]]).
