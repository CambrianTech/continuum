# Reproducible Experiments Plan for Sentinel-AI + Continuum Papers

**Date**: 2025-11-03
**Purpose**: Define reproducible experiment protocols for academic publication
**Target Venues**: NeurIPS, ICML, ICLR, AAMAS, AAAI

---

## 🎯 Research Questions

### Paper 1: Sentinel-AI Architecture
**Title**: "Adaptive Transformers with Agency-Aware Attention and Neural Plasticity"

**Core Claims to Validate**:
1. **Pruning Effectiveness**: Up to 40% of attention heads can be pruned with minimal quality impact
2. **Performance Maintenance**: Perplexity improves despite pruning (975 → 211 demonstrated)
3. **Agency Benefits**: Agency-aware attention creates compounding benefits with pruning
4. **Efficiency Gains**: 30-70% parameter reduction while maintaining/improving performance

### Paper 2: Continuum Training Infrastructure
**Title**: "Continuum: Multi-Agent Training Infrastructure for Autonomous AI Citizens"

**Core Claims to Validate**:
1. **AI-Determined Parameters**: Teacher AIs make better training decisions than hard-coded heuristics
2. **Continuous Learning**: Learning from all activities (chat, code, games) improves performance
3. **Self-Directed Learning**: AIs that create own training tasks improve faster
4. **Multi-Agent Coordination**: Recipe-based coordination outperforms single-agent training

### Paper 3: P2P Genome Sharing
**Title**: "Distributed Genome Networks: P2P Learning for AI Skill Sharing"

**Core Claims to Validate**:
1. **Discovery Effectiveness**: Performance-weighted scoring outperforms pure similarity
2. **Transfer Learning**: Downloaded genomes accelerate learning in new domains
3. **Community Evolution**: Shared genomes improve over time through collective training
4. **Scalability**: Sub-100ms similarity search across thousands of genomes

---

## 🧪 Experiment 1: Sentinel-AI Pruning Effectiveness

### Objective
Reproduce and verify the 40% pruning result from April 2025 experiments with full documentation.

### Experimental Setup

**Model**: GPT-2 (124M parameters)
**Dataset**: TinyShakespeare (1.1MB text)
**Hardware**: RTX 3060 (12GB VRAM) or equivalent
**Framework**: PyTorch + Transformers + Sentinel-AI custom layers

**Hyperparameters**:
```python
{
  "model": "gpt2",
  "dataset": "tiny_shakespeare",
  "warmup_steps": 100,
  "initial_training_steps": 500,
  "pruning_strategy": "entropy",
  "pruning_levels": [0.0, 0.1, 0.2, 0.3, 0.4, 0.5],  # Test 0-50% pruning
  "regrowth_steps": 200,
  "learning_rate": 5e-5,
  "batch_size": 8,
  "sequence_length": 128,
  "eval_samples": 100,
  "seed": 42  # For reproducibility
}
```

### Metrics to Measure

**Primary Metrics**:
- Perplexity (before/after pruning/regrowth)
- Active head count
- Training time per epoch
- GPU memory usage
- Inference speed (tokens/sec)

**Secondary Metrics**:
- Per-head entropy distribution
- Per-head gradient norms
- Agency signal distribution (active/overloaded/misaligned/withdrawn)
- Text generation quality (BLEU, repetition ratio, coherence)

### Experiment Protocol

```python
# Phase 1: Baseline Training
baseline_model = train_baseline_gpt2(
    dataset="tiny_shakespeare",
    steps=500,
    learning_rate=5e-5
)
baseline_perplexity = evaluate(baseline_model)
save_checkpoint("baseline", baseline_model)

# Phase 2: Warmup with Sentinel-AI
sentinel_model = convert_to_sentinel(baseline_model)
warmup_train(sentinel_model, steps=100)
warmup_perplexity = evaluate(sentinel_model)
save_checkpoint("warmup", sentinel_model)

# Phase 3: Progressive Pruning (0% → 50%)
results = []
for pruning_level in [0.0, 0.1, 0.2, 0.3, 0.4, 0.5]:
    # Prune heads based on entropy
    pruned_model = prune_heads(
        sentinel_model.copy(),
        strategy="entropy",
        pruning_ratio=pruning_level
    )

    # Measure immediate impact
    pruned_perplexity = evaluate(pruned_model)

    # Regrow heads strategically
    regrown_model = regrow_heads(
        pruned_model,
        steps=200,
        learning_rate=5e-5
    )

    # Measure recovery
    regrown_perplexity = evaluate(regrown_model)

    # Record all metrics
    results.append({
        "pruning_level": pruning_level,
        "active_heads": count_active_heads(regrown_model),
        "baseline_ppl": baseline_perplexity,
        "pruned_ppl": pruned_perplexity,
        "regrown_ppl": regrown_perplexity,
        "memory_mb": measure_memory(regrown_model),
        "speed_tok_sec": measure_inference_speed(regrown_model),
        "agency_distribution": get_agency_stats(regrown_model)
    })

    save_checkpoint(f"pruned_{int(pruning_level*100)}pct", regrown_model)

# Phase 4: Generate Publication Figures
generate_pruning_curves(results)
generate_agency_heatmaps(results)
generate_performance_tables(results)
save_latex_tables(results, "paper/tables/")
```

### Expected Outputs

**Data Files**:
- `results/experiment_1_baseline.json` - Baseline metrics
- `results/experiment_1_pruning_sweep.csv` - Pruning sweep data
- `results/experiment_1_agency_signals.csv` - Agency signal logs
- `models/checkpoints/` - Model checkpoints for each pruning level

**Figures** (publication-ready):
- Figure 1: Pruning vs Perplexity curve (showing minimal impact up to 40%)
- Figure 2: Active head count over training
- Figure 3: Agency signal distribution heatmap
- Figure 4: Memory usage vs pruning level
- Figure 5: Inference speed improvement

**Tables**:
- Table 1: Pruning effectiveness summary (LaTeX format)
- Table 2: Per-head entropy statistics
- Table 3: Comparison with baseline GPT-2

---

## 🧪 Experiment 2: Continuum Multi-Agent Training

### Objective
Demonstrate that Teacher AI-determined training parameters outperform hard-coded heuristics.

### Experimental Setup

**Scenario**: Code review fine-tuning for PersonaUser

**Architecture**:
- Student AI (PersonaUser being trained)
- Teacher AI (decides when/how to train)
- Peer Reviewer AI (provides feedback)
- Validator AI (measures improvement)

**Baselines to Compare**:
1. **Hard-coded heuristics**: "Train every 10 corrections with lr=0.001, epochs=3"
2. **Teacher AI decisions**: AI decides learning rate, epochs, timing, examples
3. **No coordination**: Student trains immediately on every correction

**Dataset**: 500 code review examples (TypeScript)
**Training Budget**: 2 hours GPU time per approach
**Evaluation**: Code quality metrics, review accuracy, training efficiency

### Metrics to Measure

**Performance Metrics**:
- Code review accuracy (% of issues caught)
- False positive rate
- Code quality improvement (ESLint violations reduced)
- Time to proficiency (training samples needed)

**Training Efficiency**:
- Total training time
- Number of fine-tuning runs
- GPU utilization
- Samples per fine-tuning run

**AI Decision Quality**:
- Teacher AI learning rate choices (distribution)
- Teacher AI example selection (quality scores)
- Correlation between AI decisions and outcomes

### Experiment Protocol

```typescript
// Baseline 1: Hard-coded heuristics
const hardCodedResults = await trainWithHeuristics({
  rule: "train every 10 corrections",
  learningRate: 0.001,
  epochs: 3,
  examples: allCorrections
});

// Baseline 2: No coordination (immediate training)
const immediateResults = await trainImmediate({
  trainOnEveryCorrection: true,
  learningRate: 0.001,
  epochs: 1
});

// Continuum approach: Teacher AI decides
const teacherResults = await trainWithTeacherAI({
  teacher: teacherPersona,
  student: studentPersona,
  peerReviewer: peerPersona,
  validator: validatorPersona,

  teacherPrompt: `
    Analyze student's recent performance:
    - ${recentCorrections.length} corrections accumulated
    - Recent accuracy: ${metrics.accuracy}
    - Error patterns: ${patterns}

    Should I:
    1. Let them practice more?
    2. Fine-tune now? (Choose learning rate, epochs, examples)
    3. Adjust difficulty?
    4. Request peer review first?
  `
});

// Compare all three approaches
const comparison = {
  hardCoded: evaluateApproach(hardCodedResults),
  immediate: evaluateApproach(immediateResults),
  teacherAI: evaluateApproach(teacherResults)
};

// Generate publication figures
generateTrainingCurves(comparison);
generateDecisionAnalysis(teacherResults.decisions);
generateEfficiencyComparison(comparison);
```

### Expected Outputs

**Hypothesis**: Teacher AI approach will:
- Reach proficiency 30-50% faster (fewer training samples needed)
- Use 20-40% less GPU time (fewer wasteful training runs)
- Achieve 10-20% higher accuracy (better example selection)
- Make adaptive decisions (learning rate varies based on progress)

**Data Files**:
- `results/experiment_2_hard_coded.json`
- `results/experiment_2_immediate.json`
- `results/experiment_2_teacher_ai.json`
- `results/experiment_2_teacher_decisions.csv` (AI decision log)

**Figures**:
- Figure 6: Learning curves (3 approaches)
- Figure 7: Training efficiency comparison
- Figure 8: Teacher AI decision distribution
- Figure 9: Correlation: AI decisions vs outcomes

---

## 🧪 Experiment 3: Continuous Learning Across Activities

### Objective
Demonstrate that continuous learning from multiple activity types improves performance.

### Experimental Setup

**Scenario**: PersonaUser learns from chat, code review, and game playing

**Baselines**:
1. **Single-activity training**: Only chat corrections
2. **Multi-activity training**: Chat + code + games

**Activities**:
- Chat: Conversational corrections (500 examples)
- Code: TypeScript review (300 examples)
- Games: Tic-tac-toe strategy (200 games)

**Hypothesis**: Multi-activity training creates more robust, generalizable knowledge.

### Metrics to Measure

**Generalization**:
- Chat quality after code training
- Code quality after game training
- Cross-domain transfer effectiveness

**Performance**:
- Per-activity accuracy
- Overall proficiency score
- Adaptation speed (new activity types)

### Experiment Protocol

```typescript
// Baseline: Single-activity (chat only)
const singleActivity = await trainSingleActivity({
  activity: 'chat',
  examples: chatCorrections,
  trainingRuns: 10
});

// Continuum: Multi-activity
const multiActivity = await trainMultiActivity({
  activities: ['chat', 'code', 'games'],
  examples: {
    chat: chatCorrections,
    code: codeReviews,
    games: gameMoves
  },
  trainingRuns: 10,
  interleaved: true  // Mix activities during training
});

// Test generalization
const chatAccuracy = evaluateChat(multiActivity);
const codeAccuracy = evaluateCode(multiActivity);
const gameWinRate = evaluateGames(multiActivity);

// Test transfer learning
const newActivityPerformance = evaluateOnNewActivity(
  multiActivity,
  "design-review"  // Never seen before
);
```

### Expected Outputs

**Hypothesis**: Multi-activity training will:
- Improve generalization by 15-30%
- Accelerate learning on new activity types by 40-60%
- Create more robust representations (less overfitting)

---

## 🧪 Experiment 4: P2P Genome Discovery

### Objective
Validate performance-weighted genome discovery outperforms pure similarity search.

### Experimental Setup

**Scenario**: 1000 synthetic genomes across 10 domains

**Approaches to Compare**:
1. **Pure similarity**: cosine_similarity(query, candidate) only
2. **Performance-weighted**: 50% performance, 25% similarity, 25% other
3. **Random**: Random genome selection (baseline)

**Domains**: Biology, law, medicine, finance, gaming, design, music, sports, history, chemistry

### Metrics to Measure

**Discovery Quality**:
- Top-1 accuracy (best genome found)
- Top-5 accuracy (best in top 5)
- Domain transfer success rate
- Training time reduction (vs training from scratch)

**Search Performance**:
- Query latency (ms)
- Index build time
- Memory usage
- Scalability (1k → 10k → 100k genomes)

### Experiment Protocol

```typescript
// Generate synthetic genome database
const genomes = await generateSyntheticGenomes({
  count: 1000,
  domains: 10,
  performanceDistribution: "normal",  // Mean=0.7, std=0.15
  similarityNoise: 0.1  // Embeddings not perfect
});

// Test 100 discovery queries (10 per domain)
const queries = generateQueries(domains, 10);

const results = {
  pureSimilarity: [],
  performanceWeighted: [],
  random: []
};

for (const query of queries) {
  // Pure similarity approach
  const simResults = searchGenomes(query, {
    scoring: { similarity: 1.0 }
  });

  // Performance-weighted approach
  const perfResults = searchGenomes(query, {
    scoring: {
      similarity: 0.25,
      performance: 0.50,
      availability: 0.15,
      recency: 0.05,
      community: 0.05
    }
  });

  // Random baseline
  const randomResults = randomSample(genomes, 5);

  // Evaluate transfer learning effectiveness
  results.pureSimilarity.push(evaluateTransfer(simResults[0], query));
  results.performanceWeighted.push(evaluateTransfer(perfResults[0], query));
  results.random.push(evaluateTransfer(randomResults[0], query));
}

// Generate comparison figures
generateDiscoveryComparison(results);
generateScalabilityAnalysis(genomes);
```

### Expected Outputs

**Hypothesis**: Performance-weighted scoring will:
- Improve Top-1 accuracy by 20-40%
- Reduce training time by 30-50% (better initial genomes)
- Maintain sub-100ms query latency up to 10k genomes

---

## 📊 Publication Timeline

### Phase 1: Experiment Execution (Weeks 1-2)
- Run all 4 experiments with multiple seeds (3-5 runs each)
- Collect all data and checkpoints
- Generate preliminary figures

### Phase 2: Analysis & Visualization (Week 3)
- Statistical significance testing
- Publication-quality figures (matplotlib + seaborn)
- LaTeX tables for paper

### Phase 3: Paper Writing (Weeks 4-5)
- Draft Paper 1 (Sentinel-AI architecture)
- Draft Paper 2 (Continuum infrastructure)
- ArXiv preprints

### Phase 4: Submission (Week 6)
- Submit to NeurIPS/ICML (if deadlines align)
- Otherwise: ICLR or AAMAS/AAAI

---

## 🛠️ Reproducibility Requirements

### Code Repository Structure
```
sentinel-ai/
├── experiments/
│   ├── 01_pruning_effectiveness/
│   │   ├── run_experiment.py
│   │   ├── config.json
│   │   └── README.md
│   ├── 02_multi_agent_training/
│   ├── 03_continuous_learning/
│   └── 04_p2p_discovery/
├── results/
│   ├── experiment_1/
│   │   ├── data.csv
│   │   ├── figures/
│   │   └── checkpoints/
│   └── ...
├── paper/
│   ├── figures/
│   ├── tables/
│   └── arxiv_draft_v1.tex
└── requirements-frozen.txt  # Exact versions
```

### Documentation Requirements

Each experiment must include:
1. **README.md** - Complete setup and run instructions
2. **config.json** - All hyperparameters
3. **requirements-frozen.txt** - Exact package versions
4. **run_experiment.sh** - Single-command execution
5. **generate_figures.py** - Reproduce all figures
6. **Statistical tests** - Significance testing code

### Compute Requirements

**Estimated GPU hours**:
- Experiment 1 (Pruning): 12 hours (RTX 3060)
- Experiment 2 (Multi-agent): 6 hours
- Experiment 3 (Continuous): 8 hours
- Experiment 4 (P2P): 2 hours (CPU only)

**Total**: ~30 GPU hours (~$30 on cloud, $0 on local hardware)

---

## 🎯 Success Criteria

**For Publication Acceptance**:
1. ✅ All claims backed by statistically significant results (p < 0.05)
2. ✅ Complete reproducibility (code, data, configs public)
3. ✅ Ablation studies (isolate each contribution)
4. ✅ Comparison with strong baselines
5. ✅ Clear, publication-quality figures
6. ✅ Detailed experimental protocol
7. ✅ Error bars / confidence intervals on all metrics

**For Community Impact**:
1. ✅ ArXiv preprint establishes precedence
2. ✅ GitHub repo with complete code
3. ✅ Interactive demo (Continuum + Sentinel-AI)
4. ✅ Blog post explaining key insights
5. ✅ Video walkthrough of experiments

---

## 📝 Next Steps

1. **Verify historical results**: Search for April 2025 experiment outputs
2. **Run Experiment 1**: Reproduce 40% pruning result with full documentation
3. **Generate baseline figures**: Create Figure 1-5 for Paper 1
4. **Draft Paper 1 abstract**: 250-word ArXiv abstract
5. **Setup experiment runners**: Modular Python scripts (not notebooks)

---

**Status**: Draft v1 - Ready for execution
**Last Updated**: 2025-11-03
