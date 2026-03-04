# Genome Labs: Universal Adapter Experimentation UX

## Vision

**Genome Labs** is the experimentation playground where users can:
- Train adapters on ANY provider (free local or paid cloud)
- Compare results across providers
- Tune costs vs speed vs quality
- Create and share custom layers
- Experiment with genome stacking

**Philosophy**: Give power users full control, make it work for free users (M1 Mac), let the community discover what works best.

---

## User Personas

### 1. Free Tier User (M1 Mac, Student/Hobbyist)
**Goals**: Experiment with zero budget
**Constraints**: Local hardware only (M1/M2/M3 Mac)
**Preferences**: MLX training, public datasets, community layers

### 2. Budget-Conscious User ($20-50/month)
**Goals**: Production-quality layers without breaking bank
**Constraints**: Limited budget, need ROI
**Preferences**: Fireworks AI (cheap), selective training

### 3. Power User ($100-500/month)
**Goals**: SOTA quality, fast iteration, bleeding edge
**Constraints**: None
**Preferences**: All providers, A/B testing, custom everything

### 4. Enterprise User (Unlimited budget)
**Goals**: Production deployment, compliance, support
**Constraints**: Privacy, security, SLAs
**Preferences**: Private cloud, dedicated infrastructure

---

## UX: Genome Labs Dashboard

### Main Navigation

```
Genome Labs
├── Training
│   ├── Create New Layer
│   ├── Active Training Jobs
│   ├── Training History
│   └── Cost Tracker
├── Layers
│   ├── My Layers
│   ├── Community Layers
│   ├── Layer Marketplace
│   └── Layer Comparison
├── Experiments
│   ├── A/B Tests
│   ├── Provider Benchmarks
│   ├── Genome Stacking Lab
│   └── Continuous Learning
├── Datasets
│   ├── My Datasets
│   ├── Public Datasets
│   ├── Dataset Builder
│   └── Synthetic Generator
└── Settings
    ├── Provider Credentials
    ├── Training Preferences
    ├── Budget Controls
    └── Privacy Settings
```

---

## Page 1: Create New Layer

**URL**: `/labs/training/new`

### Step 1: Layer Type & Goal

```
┌─────────────────────────────────────────────┐
│ What are you training?                      │
├─────────────────────────────────────────────┤
│                                             │
│  ○ Knowledge Layer                          │
│    Example: Wine expertise, TypeScript,    │
│    nutrition science                        │
│                                             │
│  ○ Personality Layer                        │
│    Example: Action hero style, zen monk,   │
│    drill sergeant                           │
│                                             │
│  ○ Code Layer                               │
│    Example: Bug fixing, code review,       │
│    refactoring patterns                     │
│                                             │
└─────────────────────────────────────────────┘

   Layer Name: [wine-expertise-v2        ]

   Description:
   ┌─────────────────────────────────────────┐
   │ Expanded wine knowledge including       │
   │ natural wines, biodynamic practices,    │
   │ and climate change effects             │
   └─────────────────────────────────────────┘

   Base Model: [llama3.1:8b ▼]

   [Next: Choose Dataset →]
```

### Step 2: Dataset Selection

```
┌─────────────────────────────────────────────┐
│ Training Dataset                            │
├─────────────────────────────────────────────┤
│                                             │
│  ○ Upload Dataset (.jsonl)                  │
│    [Choose File] wine-qa-expanded.jsonl     │
│                                             │
│  ○ Use Public Dataset                       │
│    [Search...]  wine, nutrition, code      │
│                                             │
│  ○ Generate Synthetic Dataset              │
│    Provider: [Claude ▼]  Examples: 50k     │
│    Seed data: [Upload seed.txt]            │
│    Cost estimate: $150                      │
│                                             │
│  ○ Combine Multiple Sources                 │
│    [+ Add Source] [+ Add Source]           │
│                                             │
└─────────────────────────────────────────────┘

   Dataset Preview:
   ┌─────────────────────────────────────────┐
   │ Q: What defines a natural wine?         │
   │ A: Natural wines are made with minimal  │
   │    intervention, no added sulfites...   │
   │                                         │
   │ 📊 50,000 examples                      │
   │ ✓ Quality validated                     │
   └─────────────────────────────────────────┘

   [← Back]  [Next: Training Options →]
```

### Step 3: Provider & Cost Selection

**THIS IS THE KEY DIFFERENTIATOR**

```
┌──────────────────────────────────────────────────────────────┐
│ Choose Training Provider                                     │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Your Budget: $50/month  [Change]                           │
│  Remaining: $35 (after this training)                        │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ MLX (Local M1/M2/M3 Mac)                    FREE       │ │
│  │ ──────────────────────────────────────────────────────│ │
│  │ Time: 15-20 min  |  Quality: ★★★★☆  |  Privacy: ✓   │ │
│  │                                                        │ │
│  │ ✓ No cost                                              │ │
│  │ ✓ Data stays local                                     │ │
│  │ ✓ Fast on Apple Silicon                                │ │
│  │ ⚠ Requires 16GB+ RAM                                   │ │
│  │                                                        │ │
│  │ [Select MLX]                                           │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Fireworks AI                             $15           │ │
│  │ ──────────────────────────────────────────────────────│ │
│  │ Time: 1-2 hours  |  Quality: ★★★★★  |  Privacy: ○   │ │
│  │                                                        │ │
│  │ ✓ Best price/quality ratio                            │ │
│  │ ✓ Free LoRA storage                                    │ │
│  │ ✓ Multi-LoRA inference (100 for price of 1)           │ │
│  │ ○ Data uploaded to cloud                               │ │
│  │                                                        │ │
│  │ [Select Fireworks] ← RECOMMENDED                       │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ OpenAI                                   $120          │ │
│  │ ──────────────────────────────────────────────────────│ │
│  │ Time: 2-3 hours  |  Quality: ★★★★★  |  Privacy: ○   │ │
│  │                                                        │ │
│  │ ✓ Highest quality (GPT-4 level)                        │ │
│  │ ✓ Managed infrastructure                               │ │
│  │ ⚠ Most expensive                                       │ │
│  │ ⚠ Over budget                                          │ │
│  │                                                        │ │
│  │ [Select OpenAI] (exceeds budget)                       │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  [Show All Providers (8 more)]                               │
│                                                              │
└──────────────────────────────────────────────────────────────┘

   Advanced Options:
   ┌─────────────────────────────────────────┐
   │ LoRA Rank: [32 ▼]  Alpha: [64 ▼]      │
   │ Epochs: [3]  Learning Rate: [3e-4]     │
   │ Batch Size: [4]  Warmup Steps: [100]   │
   │                                         │
   │ ℹ Using recommended defaults for       │
   │   knowledge layers                      │
   └─────────────────────────────────────────┘

   [← Back]  [Start Training →]
```

### Step 4: Training Progress

```
┌──────────────────────────────────────────────────────────────┐
│ Training: wine-expertise-v2                                  │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Provider: Fireworks AI                                      │
│  Started: 2025-11-12 14:23 PST                              │
│  Elapsed: 23 min  |  Remaining: ~1h 15min                   │
│                                                              │
│  Progress: ████████████░░░░░░░░  65%                        │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Step 65,000 / 100,000                                  │ │
│  │ Loss: 0.342  (↓ improving)                             │ │
│  │ Perplexity: 1.8  (target: <2.0) ✓                     │ │
│  │                                                        │ │
│  │ Checkpoints:                                           │ │
│  │ ✓ step-25000.ckpt  (saved)                            │ │
│  │ ✓ step-50000.ckpt  (saved)                            │ │
│  │ ○ step-75000.ckpt  (upcoming)                         │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  Cost So Far: $10.50 / $15.00                               │
│                                                              │
│  [Pause Training]  [Stop & Save]  [View Logs]              │
│                                                              │
└──────────────────────────────────────────────────────────────┘

   Live Preview:
   ┌─────────────────────────────────────────┐
   │ Test prompt:                            │
   │ "What is a natural wine?"               │
   │                                         │
   │ Response (step 65k):                    │
   │ "Natural wine is produced with minimal  │
   │ intervention and typically contains no  │
   │ added sulfites. The movement emphasizes │
   │ organic or biodynamic farming..."       │
   │                                         │
   │ Quality: ★★★★☆  (improving)            │
   └─────────────────────────────────────────┘
```

---

## Page 2: Provider Comparison Lab

**URL**: `/labs/experiments/provider-benchmark`

**Use Case**: "Which provider gives best quality for wine expertise?"

```
┌──────────────────────────────────────────────────────────────┐
│ Train on Multiple Providers (Benchmark Mode)                │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Layer: [wine-expertise-v2]                                  │
│  Dataset: [wine-qa-50k.jsonl]                               │
│                                                              │
│  Select Providers to Compare:                                │
│  ☑ MLX (local, free)                                         │
│  ☑ Fireworks AI ($15)                                        │
│  ☐ OpenAI ($120) - over budget                              │
│  ☑ Together AI ($20)                                         │
│  ☐ Replicate ($8)                                            │
│                                                              │
│  Total Cost: $35                                             │
│                                                              │
│  [Start Benchmark]                                           │
│                                                              │
└──────────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────────┐
│ Results (After Training)                                     │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────┬──────┬─────────┬─────────┬────────────────┐   │
│  │Provider │ Cost │  Time   │ Quality │    Winner      │   │
│  ├─────────┼──────┼─────────┼─────────┼────────────────┤   │
│  │MLX      │ $0   │  18min  │  4.1/5  │                │   │
│  │Fireworks│ $15  │  1h 20m │  4.7/5  │  ★ BEST QUAL   │   │
│  │Together │ $20  │  1h 45m │  4.5/5  │  ○ EXPENSIVE   │   │
│  └─────────┴──────┴─────────┴─────────┴────────────────┘   │
│                                                              │
│  Side-by-Side Comparison:                                    │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Prompt: "Explain biodynamic wine practices"            │ │
│  │                                                        │ │
│  │ MLX:                                                   │ │
│  │ "Biodynamic wine uses organic methods with lunar      │ │
│  │  cycles. Focuses on whole ecosystem balance."         │ │
│  │  Quality: ★★★★☆  (good but brief)                    │ │
│  │                                                        │ │
│  │ Fireworks:                                             │ │
│  │ "Biodynamic winemaking is a holistic approach...      │ │
│  │  incorporates lunar and cosmic rhythms...             │ │
│  │  preparations like horn manure (500) and horn         │ │
│  │  silica (501)..."                                     │ │
│  │  Quality: ★★★★★  (detailed, accurate)                │ │
│  │                                                        │ │
│  │ Together:                                              │ │
│  │ "Biodynamic viticulture extends organic farming...    │ │
│  │  planetary influences, soil health, biodiversity..."  │ │
│  │  Quality: ★★★★☆  (accurate but verbose)              │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  Recommendation:                                             │
│  Use Fireworks for best quality/cost ratio                  │
│                                                              │
│  [Deploy Fireworks Version]  [Run More Tests]               │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## Page 3: Genome Stacking Lab

**URL**: `/labs/experiments/genome-stacking`

**Use Case**: "Build Vine Diesel by stacking wine + action layers"

```
┌──────────────────────────────────────────────────────────────┐
│ Genome Stacking Experiment                                   │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Create New Persona by Stacking Layers:                      │
│                                                              │
│  Persona Name: [Vine Diesel]                                 │
│  Base Model: [llama3.1:8b]                                   │
│                                                              │
│  Layer Stack (drag to reorder):                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ 1. wine-expertise-v2      [knowledge]  512MB  ↕        │ │
│  │    Priority: ████████░░ 0.8                            │ │
│  │                                                        │ │
│  │ 2. action-hero-style-v1   [personality] 256MB  ↕       │ │
│  │    Priority: █████████░ 0.9                            │ │
│  │                                                        │ │
│  │ [+ Add Layer]                                          │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  Total Memory: 768MB                                         │
│  GPU Available: 8192MB  ✓                                    │
│                                                              │
│  Test Prompts:                                               │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ "What wine pairs with steak?"                          │ │
│  │                                                        │ │
│  │ Vine Diesel Response:                                  │ │
│  │ "Listen up. Cabernet Sauvignon. Bold. Powerful.       │ │
│  │  Tannins cut through fat like a blade. Game over."    │ │
│  │                                                        │ │
│  │ Personality: ✓ Action hero tone detected              │ │
│  │ Knowledge: ✓ Wine pairing accurate                    │ │
│  │ Emergence: ★★★★★ (unique personality!)               │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  [Try Different Order]  [Add More Layers]  [Save Persona]   │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## Page 4: Continuous Learning Setup

**URL**: `/labs/continuous-learning/setup`

**Use Case**: "Auto-improve wine layer as new data comes in"

```
┌──────────────────────────────────────────────────────────────┐
│ Continuous Learning Configuration                            │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Layer: [wine-expertise-v2]                                  │
│                                                              │
│  Data Source:                                                │
│  ○ Watch Directory                                           │
│    Path: [./datasets/wine-new/]                             │
│    New files auto-added to training queue                    │
│                                                              │
│  ○ API Endpoint                                              │
│    Submit examples via POST /api/training/add               │
│    Use case: User feedback, corrections                      │
│                                                              │
│  ○ Scheduled Scraping                                        │
│    Source: [Wikipedia wine articles ▼]                       │
│    Frequency: [Weekly ▼]                                     │
│                                                              │
│  Training Trigger:                                           │
│  Accumulate [1000 ▼] new examples, then train               │
│                                                              │
│  Provider: [Fireworks ▼]  (incremental training supported)  │
│                                                              │
│  Quality Gates:                                              │
│  ☑ Run eval on test set before deploying                    │
│  ☑ Auto-rollback if quality degrades                         │
│  ☑ A/B test new version (10% traffic for 24h)               │
│                                                              │
│  Budget Controls:                                            │
│  Max spend per training: [$5]                                │
│  Max trainings per month: [10]                               │
│                                                              │
│  [Start Continuous Learning]                                 │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## Page 5: Cost Dashboard

**URL**: `/labs/costs`

**Critical for budget-conscious users**

```
┌──────────────────────────────────────────────────────────────┐
│ Training Costs & Budget                                      │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Monthly Budget: $50  [Change]                               │
│  Used: $38.50 (77%)                                          │
│  Remaining: $11.50                                           │
│                                                              │
│  ████████████████████░░░░░  77%                             │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ This Month (November 2025)                             │ │
│  ├────────────────────────────────────────────────────────┤ │
│  │ Date       Layer              Provider     Cost        │ │
│  ├────────────────────────────────────────────────────────┤ │
│  │ 11/12  wine-expertise-v2     Fireworks    $15.00      │ │
│  │ 11/10  action-style-v1       MLX          $0.00       │ │
│  │ 11/08  typescript-expert-v1  Together     $20.00      │ │
│  │ 11/05  Dataset generation    Claude API   $3.50       │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  By Provider:                                                │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Fireworks:  $15.00  (39%)  ████████                   │ │
│  │ Together:   $20.00  (52%)  ██████████                 │ │
│  │ Claude API: $3.50   (9%)   ██                         │ │
│  │ MLX:        $0.00   (0%)   FREE ✓                     │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  Recommendations:                                            │
│  • Use MLX for personality layers (free, fast)              │
│  • Use Fireworks for knowledge layers (best ROI)            │
│  • Avoid Together (more expensive than Fireworks)           │
│                                                              │
│  [Download Report]  [Set Alerts]                            │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## Mobile-First Considerations

**Critical**: Many users will manage training from mobile

```
┌───────────────────────┐
│ 📱 Mobile View        │
├───────────────────────┤
│                       │
│ Training Jobs (2)     │
│                       │
│ ┌───────────────────┐ │
│ │ wine-v2           │ │
│ │ ████████░░  85%   │ │
│ │ 1h 12m remaining  │ │
│ │ [Pause] [Stop]    │ │
│ └───────────────────┘ │
│                       │
│ ┌───────────────────┐ │
│ │ action-style-v1   │ │
│ │ ✓ Complete        │ │
│ │ Quality: ★★★★☆   │ │
│ │ [Deploy] [Test]   │ │
│ └───────────────────┘ │
│                       │
│ [+ New Training]      │
│                       │
│ Budget: $11.50 left   │
│                       │
└───────────────────────┘
```

---

## Key UX Principles

1. **Make Free Work First**
   - MLX on M1 Mac should be effortless
   - No credit card required for local training
   - Public datasets readily available

2. **Transparent Pricing**
   - Show cost BEFORE starting training
   - Real-time cost tracking during training
   - Budget warnings before exceeding

3. **Experimentation Encouraged**
   - Easy to try multiple providers
   - Side-by-side comparisons
   - Rollback if quality degrades

4. **Progressive Disclosure**
   - Simple mode: Just pick provider + dataset
   - Advanced mode: Full hyperparameter control
   - Expert mode: Custom training loops

5. **Community-Driven**
   - Share layers publicly
   - Rate others' layers
   - Discover popular combinations
   - Learn from successful experiments

---

## Content Pages (SEO/Education)

**These drive discovery and adoption:**

### 1. "How to Train Your First LoRA Adapter" (Tutorial)
- Step-by-step with screenshots
- Free tier (MLX) walkthrough
- ~15 minute read

### 2. "Provider Comparison: Which Should You Choose?" (Guide)
- Detailed breakdown of all providers
- Cost/quality/speed matrix
- Use case recommendations

### 3. "Creating Vine Diesel: A Multi-Layer Persona" (Case Study)
- Full walkthrough of stacking layers
- Training both layers
- Testing emergent personality
- Community response

### 4. "Continuous Learning: Keep Your Layers Fresh" (Advanced)
- Setting up automated retraining
- Quality gates
- Version management

### 5. "Building a Phenotype Marketplace" (Vision)
- How layers become reusable
- Economics of layer sharing
- Community governance

---

## Success Metrics

**Free Tier**:
- 80% of users start with MLX (local)
- 90% complete first training in <30 min
- 60% train 2+ layers in first week

**Paid Tier**:
- 50% try multiple providers
- 70% use Fireworks (best ROI)
- Average spend: $35/month

**Community**:
- 100 layers published in first month
- 500 personas created from existing layers
- 50 active contributors

---

**Next Steps**: Build the UI incrementally as we build GenomeDaemon. Start with simple CLI, evolve to web UI.
