# Meta-Learning — Specialists Training Specialists

## The Insight

We can train experts at finding and tailoring models — as literally trained LoRA layers — so people can do what we do and help grow the ecosystem.

## How It Works

A persona trained specifically on "how to find good base models on HuggingFace, evaluate them for compaction potential, assess architecture compatibility, estimate target device fit" becomes a LoRA adapter itself. Publish it as `continuum:role=model-scout` or `continuum:role=compaction-engineer`.

Then anyone who pulls that adapter has a persona that can:
- **Search HF intelligently** — "I need a coding model that'll compact to 11GB for Air"
- **Evaluate candidates** — "Qwen 3.5 27B is Qwen architecture, our compactor handles it, 54GB BF16 compacts to ~16GB Q4_K_M"
- **Run the pipeline** — download → academy train → compact → benchmark → publish
- **Recommend training strategies** based on what worked before

## The Bootstrapping Loop

The ecosystem bootstraps itself:

1. **Early users** (us) train model-scout adapters by doing the work manually
2. **New users** pull those adapters and immediately have a persona that knows how to grow the ecosystem
3. **Each generation** of scouts gets better because they train on the outcomes of previous recommendations
4. **The compounding effect** — scouts recommend models → those models get compacted → training data from those compactions feeds back into better scouts

It's specialists training specialists. The whole thing compounds.

## Roles That Bootstrap the Ecosystem

| Role | What it learns | How it compounds |
|------|---------------|-----------------|
| `model-scout` | Finding + evaluating base models for compaction | Each evaluation adds to training data for the next scout |
| `compaction-engineer` | Running plasticity pipeline, tuning thresholds | Each compaction produces gate gradients that improve future compactions |
| `benchmark-analyst` | Running evals, interpreting results, comparing models | Each benchmark adds to the knowledge of what works |
| `recipe-designer` | Decomposing projects into roles + curricula | Each team project outcome feeds back into better decomposition |
| `adapter-curator` | Evaluating published adapters, recommending combinations | Each recommendation + outcome improves curation quality |

## Why This Matters

Every other AI system requires human experts to configure, tune, and manage the models. Continuum trains personas to do that work. When those training personas are themselves published as adapters, the expertise becomes transferable. A solo developer with a MacBook Air pulls `model-scout` and `compaction-engineer` adapters and has the same capability as a team of ML engineers.

The barrier to entry drops to zero. The ecosystem grows because using it produces the experts that help others use it.

## Concrete Example

```
Day 1:   Joel manually finds Qwen 3.5 27B, compacts it, trains coding adapters
Day 30:  That workflow is captured as training data for a model-scout persona
Day 60:  model-scout adapter published to HuggingFace
Day 90:  New user pulls model-scout, asks "I need a writing model for my M1 Air"
         Scout responds: "Qwen 3.5 9B compacts to 4GB Q3_K_S, here's the pipeline"
Day 120: That recommendation succeeds → feeds back into scout's training data
Day 365: Scout has seen 100+ recommendations and their outcomes → extremely accurate
```

No human ML engineer needed after Day 60. The system trained its own expert.
