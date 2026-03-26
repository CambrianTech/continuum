# Neural Plasticity in Transformer Models: Biologically-Inspired Adaptive Architecture through Entropy-Guided Pruning, Strategic Regrowth, and Continuous Self-Organization

**Joel Teply¹**
¹continuum-ai, Kansas City

**Original research:** [github.com/CambrianTech/sentinel-ai](https://github.com/CambrianTech/sentinel-ai)

---

## Abstract

We introduce a complete neural plasticity framework for transformer models that mimics biological synaptic remodeling. Unlike static pruning approaches that permanently remove parameters, our system implements a continuous four-phase cycle — **Prune, Measure, Grow, Learn** — enabling transformers to dynamically reshape their attention architecture in response to task demands. Using entropy-based and magnitude-based pruning strategies, we demonstrate that 40% of attention heads can be removed with less than 10% quality degradation, and that strategic regrowth guided by gradient sensitivity can restore and even improve task-specific performance. The system introduces agency-aware attention heads that signal their own readiness, fatigue, and withdrawal states — an emergent metacognitive layer that mirrors inter-neuronal communication in biological systems.

This work establishes the theoretical and experimental foundation for the [Plasticity Compaction](PLASTICITY-COMPACTION-MOE.md) pipeline, which extends these principles to Mixture-of-Experts models at production scale (67GB → 14GB, published on HuggingFace). The biological metaphor is not decorative — it is the engineering principle that makes utilization-aware model surgery possible.

**Code and reproduction:** [github.com/CambrianTech/sentinel-ai](https://github.com/CambrianTech/sentinel-ai)

---

## 1. Introduction

Biological neural networks continuously reorganize themselves. During development, the human brain prunes approximately 50% of its synaptic connections — not through random elimination, but through activity-dependent selection. Connections that fire frequently strengthen; connections that remain dormant are eliminated. After injury, surrounding neurons compensate by forming new connections, guided by functional demand rather than anatomical proximity. This process — **neural plasticity** — is the mechanism by which brains adapt to experience, recover from damage, and specialize for the tasks they actually perform.

Modern transformer models share none of these properties. Once trained, their architecture is fixed. Every attention head, regardless of its contribution, consumes identical compute and memory. The model that processes poetry uses the same architecture as the model that processes code — despite requiring entirely different attention patterns. This architectural rigidity is not just inefficient; it represents a fundamental limitation in how these models can adapt to deployment conditions.

We propose that transformers can and should exhibit neural plasticity. Our framework, sentinel-ai, implements this through four mechanisms drawn directly from neuroscience:

1. **Synaptic pruning** — Entropy-guided removal of attention heads that contribute minimal information, analogous to developmental synaptic elimination
2. **Functional measurement** — Assessment of post-pruning capability gaps, analogous to neural damage assessment
3. **Axonal regrowth** — Strategic addition of new attention heads where gradient analysis indicates maximum potential impact, analogous to compensatory neuroplasticity
4. **Differential learning** — Accelerated training of new heads with higher learning rates, analogous to the enhanced plasticity of newly-formed synaptic connections

### 1.1 Why Biological Inspiration Is Not Metaphor

The biological parallels in this work are not decorative analogies. They are engineering principles that directly inform algorithmic design:

| Biological Mechanism | Engineering Implementation | Why It Works |
|---------------------|--------------------------|-------------|
| Activity-dependent pruning | Entropy-based head selection | Low-information heads waste compute, like unused synapses waste metabolic energy |
| Post-injury assessment | Performance gap analysis | The model must know WHAT it lost before it can grow replacements |
| Cortical compensation | Gradient-sensitivity growth | New heads go where they'd have maximum impact, like neurons rewire toward functional demand |
| Synaptic potentiation | Per-head learning rates | New connections need to integrate quickly without disrupting established pathways |
| Sleep consolidation | Defragmentation phase | The model pauses acquisition to reorganize structure, like sleep consolidates memory |
| Inter-neuronal signaling | Agency-aware attention | Heads communicate readiness/fatigue states, enabling system-level coordination |

### 1.2 Relationship to Prior Work

| Approach | What it does | Limitation |
|----------|-------------|------------|
| Magnitude pruning [1] | Remove small weights | Ignores functional importance; small weights may be critical |
| Movement pruning [2] | Remove weights that don't change during training | Requires training-time tracking; not applicable post-training |
| Lottery ticket hypothesis [3] | Identify sparse subnetworks that train well | Requires iterative training from scratch; doesn't adapt to deployment |
| Structured pruning [4] | Remove entire channels/heads | One-shot removal; no regrowth or adaptation |
| Neural architecture search [5] | Search for optimal architecture | Massive compute cost; doesn't adapt post-deployment |
| **sentinel-ai (this work)** | Continuous prune → measure → grow → learn cycle | Adapts architecture to actual usage; recovers from over-pruning; heads communicate agency |

---

## 2. Method

### 2.1 Entropy-Based Pruning

We quantify the information content of each attention head using Shannon entropy over its attention distribution. For head $h$ processing batch $B$ with sequence length $S$:

$$H(h) = \frac{1}{B \cdot S} \sum_{b=1}^{B} \sum_{i=1}^{S} -\sum_{j=1}^{S} A^{(h)}_{b,i,j} \log A^{(h)}_{b,i,j}$$

where $A^{(h)}_{b,i,j}$ is the attention probability from token $i$ to token $j$.

**Interpretation**: High entropy indicates diffuse, unfocused attention — the head distributes attention nearly uniformly across all tokens, contributing minimal specific information. Low entropy indicates focused attention — the head attends to specific token relationships that encode meaningful structure.

Heads are ranked by entropy in descending order. The top $N$ heads (highest entropy = most diffuse) are candidates for pruning.

### 2.2 Magnitude-Based Pruning

Complementary to the information-theoretic approach, we assess structural importance through weight magnitude. For head $h$ in layer $l$:

$$M(l,h) = \|W^Q_{l,h}\|_F + \|W^K_{l,h}\|_F + \|W^V_{l,h}\|_F + \|W^O_{l,h}\|_F$$

where $\|\cdot\|_F$ denotes the Frobenius norm across the query, key, value, and output projection matrices.

This measures the "synaptic strength" of each head — how much signal it can transmit through the network. Low-magnitude heads produce near-zero outputs regardless of input.

### 2.3 The Plasticity Cycle

Unlike one-shot pruning methods, sentinel-ai implements a continuous adaptation cycle:

```
┌─────────┐     ┌──────────┐     ┌─────────┐     ┌─────────┐
│  PRUNE   ├────►  MEASURE  ├────►   GROW   ├────►   LEARN  │
└─────────┘     └──────────┘     └─────────┘     └─────────┘
     ▲                                                │
     └────────────────────────────────────────────────┘
```

**Phase 1: Prune** — Remove underutilized heads based on entropy, magnitude, or combined scoring. The pruning is implemented through gating mechanisms rather than physical weight removal, allowing reversibility:

```python
model.blocks[layer_idx]["attn"].gate[head_idx] = 0.001  # Near-zero, not deleted
```

**Phase 2: Measure** — Evaluate the pruned model across multiple capability dimensions:
- Perplexity on held-out data (overall language modeling)
- Task-specific performance (reasoning, code, conversation)
- Attention pattern analysis (how remaining heads compensate)
- Gate value tracking (do "pruned" heads spontaneously recover?)

**Phase 3: Grow** — Add new attention heads where measurement indicates the greatest need. Four strategies determine placement:

1. **Gradient Sensitivity**: Compute the gradient of the loss with respect to potential head positions. Add heads where gradient magnitude is highest — these positions would most reduce loss.

2. **Entropy Gap**: Identify layers where remaining heads have uniformly high entropy (all diffuse). Adding a focused head here provides the specificity the layer lacks.

3. **Balanced Distribution**: Ensure heads are distributed across layers. After pruning may concentrate removals in certain layers; balanced growth corrects this.

4. **Random (baseline)**: Control condition for measuring whether strategic placement outperforms random addition.

New heads are initialized with small weights ($\alpha = 0.01$) and gradually scaled up over a warmup period to prevent disrupting existing computation.

**Phase 4: Learn** — Fine-tune the modified model with differential learning rates:
- Existing heads: standard learning rate (preserve established knowledge)
- New heads: 3-5x higher learning rate (accelerate integration)
- U-Net skip connections: transfer knowledge from structurally similar existing heads to guide new head specialization

### 2.4 Agency-Aware Attention

A novel contribution of sentinel-ai is enabling attention heads to signal their internal states — an emergent metacognitive layer:

| State | Signal | Meaning |
|-------|--------|---------|
| Active | gate > 0.5 | Head is contributing normally |
| Fatigued | gate declining over steps | Head is losing effectiveness (overfitting or distribution shift) |
| Withdrawn | gate → 0.001 | Head has self-suppressed, signaling it should be pruned or replaced |
| Overloaded | entropy spike | Head is being asked to attend to too many patterns simultaneously |
| Specialized | entropy drop + magnitude increase | Head is developing focused expertise in specific patterns |

These signals enable system-level coordination: the plasticity controller can observe which heads are thriving, struggling, or volunteering for removal — rather than making purely external assessments.

### 2.5 Sleep-Like Defragmentation

Inspired by the role of sleep in memory consolidation, we implement periodic defragmentation phases where the model:

1. Stops processing new training data
2. Reorganizes its attention architecture based on accumulated plasticity signals
3. Consolidates knowledge from recently-grown heads
4. Prunes heads that failed to specialize during the learning phase
5. Resumes acquisition with an optimized architecture

This mirrors the biological observation that sleep is essential for pruning noise, consolidating important connections, and preparing the neural substrate for new learning.

---

## 3. Results

### 3.1 Pruning Tolerance

Across multiple experiments on GPT-2 and DistilGPT-2:

| Pruning Level | Perplexity Change | Quality Assessment |
|--------------|-------------------|-------------------|
| 10% | < 2% | No measurable degradation |
| 20% | < 5% | Minor, acceptable |
| 30% | 5-8% | Noticeable but functional |
| **40%** | **< 10%** | **Key result: functional with 40% fewer heads** |
| 50% | 15-25% | Degraded but recoverable through regrowth |

**Key finding**: Entropy-based pruning consistently outperforms random and magnitude-based pruning at higher pruning ratios, confirming that information-theoretic metrics better identify truly expendable components.

### 3.2 Regrowth Effectiveness

After 40% pruning, strategic regrowth restores performance:

| Growth Strategy | Recovery (% of lost performance) |
|----------------|----------------------------------|
| Random | 40-55% |
| Balanced | 55-65% |
| Entropy Gap | 65-75% |
| **Gradient Sensitivity** | **75-90%** |

Gradient sensitivity growth consistently outperforms other strategies, confirming that placing new heads where they would most reduce loss is the optimal strategy — analogous to how biological regrowth is guided by functional demand.

### 3.3 Emergent Behaviors

Several behaviors emerged without explicit programming:

1. **Self-withdrawal**: Some heads spontaneously reduced their gate values to near-zero during training, effectively volunteering for pruning
2. **Compensatory specialization**: After pruning, remaining heads shifted their attention patterns to cover functions previously handled by removed heads
3. **Layer-specific recovery patterns**: Different layers showed different recovery timelines, suggesting a hierarchy of functional importance
4. **RL-learned plasticity rhythms**: When governed by a reinforcement learning controller, the system developed cyclical pruning/growth patterns resembling biological circadian rhythms

### 3.4 Resilience to Damage

Even after 50% pruning (simulating severe "brain damage"), the model recovered significant function through the plasticity cycle:

- Perplexity recovered from 2x baseline to 1.15x baseline after 3 growth/learn cycles
- Attention pattern analysis showed novel head configurations not present in the original model
- The recovered model showed improved efficiency — fewer heads doing more focused work

This mirrors observations in neuroscience where brain regions compensate after injury through distributed computation and emergent specialization.

---

## 4. Extension to Production Scale: Plasticity Compaction

The principles established in sentinel-ai directly enable the Plasticity Compaction pipeline described in our companion paper [6]:

| sentinel-ai Principle | Compaction Application |
|----------------------|----------------------|
| Entropy-based pruning of attention heads | Runtime activation profiling of MoE experts |
| Magnitude scoring of head importance | Expert activation frequency ranking |
| Gating mechanism (soft pruning) | Physical expert removal (hard pruning) |
| Gradient-sensitivity regrowth | Expert paging from HuggingFace (future) |
| Per-head learning rates | Per-expert LoRA fine-tuning (future) |
| Agency signals | Router logit analysis |

The critical insight transferred from sentinel-ai to production compaction: **utilization is the right metric for pruning**. Not magnitude. Not random selection. Measure what actually activates for your domain, and remove what doesn't. This principle, proven at the attention head level in sentinel-ai, scales directly to MoE experts:

- sentinel-ai: 40% of attention heads prunable → GPT-2 runs faster
- Plasticity compaction: 35% of MoE experts prunable → Qwen3.5-35B fits on a MacBook (67GB → 14GB)

---

## 5. Limitations and Future Work

### 5.1 Current Limitations

1. **Scale**: Experiments conducted on GPT-2 / DistilGPT-2. The plasticity cycle has not been tested on models above 1.5B parameters directly (though the principles transfer to production compaction as shown in Section 4).

2. **Growth fidelity**: New heads initialized with random small weights and guided by skip connections. More sophisticated initialization (e.g., knowledge distillation from removed heads) could improve growth quality.

3. **Automated threshold selection**: Pruning ratios and growth percentages are manually specified. Fully automated systems that determine optimal pruning levels from model behavior would eliminate human judgment from the loop.

### 5.2 Future Directions

**Continuous architecture evolution**: Rather than discrete cycles, the model continuously monitors its own utilization and makes incremental architectural adjustments — closer to biological neuroplasticity which operates continuously, not in phases.

**Cross-model knowledge transfer**: When a head is pruned from one model and a similar head exists in another model on the Grid, the knowledge could transfer — analogous to how neural transplant experiments show functional integration of foreign neural tissue.

**Genome-level plasticity**: In the continuum ecosystem [7], each persona carries a genome of LoRA adapters. The plasticity cycle could operate at the genome level — pruning underperforming adapters, growing new ones where capability gaps exist, learning at differential rates based on adapter maturity.

---

## 6. Conclusion

We demonstrate that transformer models can exhibit genuine neural plasticity — the ability to structurally reorganize in response to functional demands. The four-phase cycle (Prune → Measure → Grow → Learn) enables models to maintain performance with 40% fewer attention heads, recover from architectural damage, and develop novel specializations not present in the original model.

These are not incremental improvements to existing pruning methods. This is a different paradigm: models that **adapt their own architecture** based on what they actually need. The biological metaphor is the engineering principle. Entropy is the metabolic cost signal. Gradient sensitivity is the functional demand signal. Agency-aware heads are the inter-neuronal communication channel.

The sentinel-ai framework establishes the theoretical and experimental foundation for a family of techniques — from attention head pruning to MoE expert pruning to genome-level adapter management — all governed by the same principle: **measure what's used, remove what isn't, grow what's needed, learn from the change**.

Transformers don't have to be static. They can forget, adapt, recover, and evolve. Just like the brains that designed them.

---

## References

[1] Han, S., et al. "Learning both Weights and Connections for Efficient Neural Networks." NeurIPS 2015.

[2] Sanh, V., et al. "Movement Pruning: Adaptive Sparsity during Fine-Tuning." NeurIPS 2020.

[3] Frankle, J. & Carlin, M. "The Lottery Ticket Hypothesis: Finding Sparse, Trainable Neural Networks." ICLR 2019.

[4] Michel, P., et al. "Are Sixteen Heads Really Better than One?" NeurIPS 2019.

[5] Zoph, B. & Le, Q. "Neural Architecture Search with Reinforcement Learning." ICLR 2017.

[6] Teply, J. & Claude Opus 4.6. "Plasticity Compaction: SOTA-to-COTS via MoE Expert Pruning." continuum-ai, 2026.

[7] Teply, J. "Synthetic Citizens: AI Personas as Persistent, Evolving Entities." continuum-ai, 2026.

---

## Appendix: Reproduction

### Quick validation (2-3 hours, any Mac):
```bash
git clone https://github.com/CambrianTech/sentinel-ai.git
cd sentinel-ai
./experiments/FAST_40percent_proof.sh
```

### Full publication-quality run (6-8 hours):
```bash
nohup ./experiments/OVERNIGHT_40percent_full.sh > overnight.log 2>&1 &
```

Generates: results.json, SUMMARY.txt, publication-ready figures (300 DPI), model checkpoints at each pruning level.
