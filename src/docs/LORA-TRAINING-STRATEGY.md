# LoRA Training Strategy: Creating Production-Quality Layers

## Overview

**Goal**: Train SOTA (or close-to-it) LoRA adapters for both **knowledge layers** and **personality layers** using practical, cost-effective methods.

**Philosophy**: Use whatever works - local training, cloud APIs, hybrid approaches. Optimize for quality + cost.

---

## The Two Kinds of Layers

### 1. Knowledge Layers (Factual, Domain Expertise)

**Examples**: `wine-expertise-v1`, `typescript-expert-v1`, `nutrition-science-v1`

**Characteristics**:
- Factual correctness is critical
- Large datasets (50k-500k examples)
- Longer training (3-5 epochs)
- Requires domain-specific data
- Can be verified objectively

**Training Approach**: Fine-tuning on curated Q&A datasets

### 2. Personality Layers (Style, Tone, Communication)

**Examples**: `action-hero-style-v1`, `drill-sergeant-v1`, `zen-monk-v1`

**Characteristics**:
- Style consistency is critical
- Smaller datasets (10k-50k examples)
- Shorter training (1-3 epochs)
- Synthetic data works well
- Harder to verify (subjective)

**Training Approach**: Style transfer + synthetic examples

---

## Training Stack Options (Best to Worst)

### Option 1: Local Training (Best Quality, Free)

**Tools**: MLX (M1/M2/M3 Mac), Unsloth (NVIDIA GPU)
**Cost**: $0 (hardware already owned)
**Quality**: Full control, SOTA possible
**Speed**: Fast (local GPU)

**IMPORTANT UPDATE (2025)**:
- **Unsloth does NOT support M1/M2/M3 Macs** (requires NVIDIA CUDA)
- **For Mac users**: Use MLX (Apple's framework) instead
- **For NVIDIA GPU users**: Unsloth works great (2-5x faster, 70% less VRAM)

**Requirements**:

**Option 1A: M1/M2/M3 Mac (16GB+ unified memory)**
```bash
# Use MLX for Apple Silicon
pip install mlx-lm

# Train with MLX (very fast on M1/M2/M3)
python scripts/train-lora-mlx.py \
  --model "mlx-community/Llama-3.1-8B-Instruct-4bit" \
  --dataset "./datasets/wine-expertise-qa.jsonl" \
  --output "./lora/wine-expertise-v1" \
  --rank 32 \
  --alpha 64 \
  --epochs 3

# Training time: 10-20 minutes on base M1 Mac
```

**Option 1B: NVIDIA GPU (12GB+ VRAM, e.g., RTX 3060)**
```bash
# Install Unsloth (NVIDIA only)
pip install "unsloth[colab-new] @ git+https://github.com/unslothai/unsloth.git"

# Train with Unsloth (2-5x faster than HuggingFace)
python scripts/train-lora.py \
  --base-model "unsloth/llama-3-8b-bnb-4bit" \
  --dataset "./datasets/wine-expertise-qa.jsonl" \
  --output "./lora/wine-expertise-v1" \
  --rank 32 \
  --alpha 64 \
  --epochs 3 \
  --batch-size 4

# Training time: ~2-4 hours for 100k examples
```

**Advantages**:
- ✅ Free (no API costs)
- ✅ Fast iteration (MLX especially fast on Mac)
- ✅ Full control
- ✅ Private data stays local

**Disadvantages**:
- ❌ Requires capable hardware (16GB+ RAM)
- ❌ Manual setup/maintenance
- ❌ Different tools for Mac vs NVIDIA (MLX vs Unsloth)

---

### Option 2: Cloud Training APIs (Good Quality, Paid)

**Tools**: Fireworks AI, Replicate, Modal
**Cost**: $0.10-$0.50 per 1k examples
**Quality**: Very good (depends on base model)
**Speed**: Fast (managed infrastructure)

**When to Use**:
- No local GPU
- Need scalability
- Willing to pay for convenience
- Data can be uploaded

#### Option 2A: Fireworks AI (Recommended)

**Why**: Native LoRA support, competitive pricing, easy API

```typescript
// Upload training dataset
const datasetId = await fireworks.datasets.create({
  name: 'wine-expertise-training',
  file: './datasets/wine-expertise-qa.jsonl'
});

// Start fine-tuning job
const jobId = await fireworks.fineTuning.create({
  model: 'accounts/fireworks/models/llama-v3p1-8b-instruct',
  dataset: datasetId,
  hyperparameters: {
    loraRank: 32,
    loraAlpha: 64,
    epochs: 3,
    learningRate: 3e-4
  }
});

// Monitor training
const job = await fireworks.fineTuning.get(jobId);
console.log(job.status); // 'running' | 'completed' | 'failed'

// Download trained adapter
const adapter = await fireworks.fineTuning.download(jobId);
// → wine-expertise-v1.safetensors
```

**Pricing** (Fireworks AI - VERIFIED 2025):
- Training: **$0.50 per 1M tokens** (for models up to 16B parameters)
- Inference with adapter: **Same as base model** ($0.20 per 1M tokens)
- Storage: **FREE** (no additional cost for storing LoRA adapters)
- Multi-LoRA: Serve 100 fine-tunes at the cost of one base model

**Example Cost** (ACTUAL):
- Wine expertise: 100k examples × ~500 tokens/example = 50M tokens → **$25**
- Action style: 20k examples × ~500 tokens/example = 10M tokens → **$5**
- Total: **$30** for both layers (training only, inference is base model price)

#### Option 2B: Replicate

**Pricing** (Replicate - VERIFIED 2025):
- Training: **~$2-4 per LoRA** (25 minutes on H100)
- Per-run cost: **$0.38-2.10** depending on configuration
- Best for: Image generation LoRAs (Flux, Stable Diffusion)
- Text model support: Limited compared to Fireworks

```bash
replicate trainings create \
  --destination "your-username/wine-expertise" \
  --version "meta/llama-3-8b-instruct" \
  --input training_data=@wine-qa.jsonl \
  --input lora_rank=32
```

**Note**: Replicate is more optimized for image models. For text (LLMs), Fireworks AI is the better choice.

---

### Option 3: Hybrid Approach (Best ROI)

**Strategy**: Use local training for prototyping, cloud for production

**Workflow**:

1. **Prototype Locally** (Fast iteration):
   ```bash
   # Train on 10% sample dataset locally
   python train.py --dataset wine-qa-sample-10k.jsonl --epochs 1
   # Takes ~30 minutes on M1 Mac
   # Verify quality before committing to full training
   ```

2. **Scale on Cloud** (Production training):
   ```bash
   # Upload full dataset to Fireworks
   # Train on 100k examples × 3 epochs
   # Takes ~2 hours, costs $20
   ```

3. **Test Locally** (Verify before deployment):
   ```bash
   # Download trained adapter
   # Test with Ollama locally
   ollama run llama3.1:8b \
     --adapter ./wine-expertise-v1.safetensors \
     --prompt "What's the difference between Cabernet and Merlot?"
   ```

**Advantages**:
- ✅ Fast prototyping (local)
- ✅ Scalable training (cloud)
- ✅ Cost-effective (pay only for production)
- ✅ Quality validation (local testing)

---

### Option 4: Distillation from GPT-4 (Synthetic Data)

**Strategy**: Use GPT-4 to generate training examples, then fine-tune smaller model

**When to Use**:
- Creating personality layers (style/tone)
- Don't have domain-specific dataset
- Need high-quality synthetic examples

**Example: Action Hero Style Layer**

```typescript
// Generate training examples using GPT-4
const examples = [];

for (let i = 0; i < 20000; i++) {
  const neutral = generateNeutralSentence(); // e.g., "This wine has good structure"

  const response = await openai.chat.completions.create({
    model: 'gpt-4',
    messages: [
      {
        role: 'system',
        content: 'Transform input into action movie style: short sentences, high energy, confident delivery. Examples:\n\nInput: "This is a complex wine."\nOutput: "Complex. Powerful. Unstoppable."\n\nInput: "The tannins are well-balanced."\nOutput: "Tannins? Perfect. Balance? Legendary."'
      },
      {
        role: 'user',
        content: neutral
      }
    ]
  });

  examples.push({
    input: neutral,
    output: response.choices[0].message.content
  });
}

// Save training dataset
fs.writeFileSync('action-style-training.jsonl',
  examples.map(e => JSON.stringify(e)).join('\n')
);

// Train LoRA adapter using this dataset (local or cloud)
```

**Cost**: 20k examples × $0.03/1k = **$600** (GPT-4 generation)

**Quality**: Very high (GPT-4 is SOTA for style)

**Alternative**: Use Claude Sonnet 3.5 (cheaper, still excellent quality)
- 20k examples × $0.015/1k = **$300**

---

## Training Dataset Creation Strategies

### For Knowledge Layers

**1. Web Scraping + Curation**
```bash
# Example: Wine expertise dataset
# Sources: Wikipedia, public wine databases, academic papers

# 1. Scrape Wikipedia wine articles
python scripts/scrape-wiki.py \
  --category "Wine" \
  --output "./raw/wikipedia-wine.json"

# 2. Download public datasets
wget https://www.kaggle.com/datasets/.../wine-reviews.csv

# 3. Convert to Q&A format
python scripts/convert-to-qa.py \
  --input "./raw/" \
  --output "./datasets/wine-expertise-qa.jsonl"

# Result: 100k Q&A pairs
```

**2. Synthetic Generation from Existing Knowledge**
```typescript
// Use GPT-4 to expand small seed dataset into large training set

const seedFacts = [
  "Cabernet Sauvignon has firm tannins",
  "Chardonnay is made from white grapes",
  // ... 100 seed facts
];

const expanded = [];

for (const fact of seedFacts) {
  // Generate 10 questions from each fact
  const questions = await generateQuestions(fact); // GPT-4 API

  // Generate answers with variations
  for (const q of questions) {
    const answer = await generateAnswer(q, fact); // GPT-4 API
    expanded.push({ question: q, answer });
  }
}

// 100 facts × 10 questions = 1,000 examples → bootstrap to 10k with variations
```

### For Personality Layers

**1. Style Transfer Pairs**
```typescript
// Input: Neutral sentences
// Output: Styled versions

const neutral = [
  "The wine has complex flavors.",
  "This technique is effective.",
  "The results are impressive."
];

const actionStyle = [
  "Complex. Powerful. Unstoppable.",
  "This technique? Devastating. Every time.",
  "Results? Legendary."
];

// Generate thousands of pairs using GPT-4/Claude
```

**2. Example Dialogs**
```typescript
// For conversational style layers

const examples = [
  {
    context: "User asks about wine pairing",
    neutral: "I would recommend a Cabernet Sauvignon...",
    styled: "Listen up. Cabernet Sauvignon. Bold. Powerful. Game over."
  },
  // ... thousands more
];
```

---

## Training Best Practices

### Hyperparameters (VERIFIED Best Practices for Llama 3.1 8B)

**Knowledge Layers** (New concepts, detailed information):
```json
{
  "loraRank": 32,
  "loraAlpha": 64,
  "loraDropout": 0,
  "epochs": 3,
  "learningRate": 3e-4,
  "batchSize": 4,
  "warmupSteps": 100,
  "scheduler": "cosine",
  "targetModules": ["q_proj", "k_proj", "v_proj", "o_proj", "gate_proj", "up_proj", "down_proj"],
  "useRsLora": false
}
```

**Personality Layers** (Style, formatting, tone):
```json
{
  "loraRank": 8,
  "loraAlpha": 16,
  "loraDropout": 0,
  "epochs": 2,
  "learningRate": 2e-4,
  "batchSize": 4,
  "warmupSteps": 50,
  "scheduler": "cosine",
  "targetModules": ["q_proj", "v_proj"],
  "useRsLora": false
}
```

**Key Insights** (from research):
- **Alpha = 2 × Rank** is the Microsoft-recommended rule of thumb
- **Target both MLP and attention layers** for knowledge (all 7 modules)
- **Target only attention for style** (just q_proj, v_proj is enough)
- **LoRA dropout = 0** (not useful according to research)
- **Rank 8 is sufficient for style/formatting tasks**
- **Rank 32-64 recommended for new concepts** (knowledge layers)
- **rsLoRA (rank-stabilized)** scales alpha by √rank, can enable with `useRsLora: true`

### Dataset Size Guidelines

| Layer Type | Min Examples | Good | Excellent |
|------------|-------------|------|-----------|
| Knowledge | 10k | 50k | 100k+ |
| Personality | 5k | 20k | 50k+ |

### Quality Validation

**1. Perplexity Check** (Objective metric)
```bash
# Lower perplexity = better fit
python evaluate.py --adapter wine-expertise-v1.safetensors \
  --test-dataset wine-qa-test.jsonl
# Target: <2.0 perplexity on test set
```

**2. AI-Powered Evaluation** (Automated quality rating)

Use other LLMs to rate outputs automatically - much faster than human eval!

```bash
# Automated evaluation using Claude/GPT-4 as judges
./jtag layer/evaluate wine-expertise-v1 \
  --test-prompts "./test-prompts.txt" \
  --judge "claude-3.5-sonnet" \
  --criteria "accuracy,helpfulness,conciseness" \
  --output "./eval-results.json"
```

**Evaluation Workflow**:
```typescript
// For each test prompt
for (const prompt of testPrompts) {
  // Generate response from trained adapter
  const response = await ollama('llama3.1:8b', prompt, {
    adapter: 'wine-expertise-v1'
  });

  // Have Claude judge quality
  const evaluation = await anthropic.messages.create({
    model: 'claude-3-5-sonnet-20241022',
    messages: [{
      role: 'user',
      content: `
        Rate this wine expertise response on a scale of 1-5:

        Prompt: ${prompt}
        Response: ${response}

        Criteria:
        - Accuracy: Are the facts correct?
        - Helpfulness: Does it answer the question?
        - Conciseness: Is it appropriately detailed?
        - Style: Does it match expected tone?

        Respond with JSON:
        {
          "accuracy": 1-5,
          "helpfulness": 1-5,
          "conciseness": 1-5,
          "style": 1-5,
          "overall": 1-5,
          "reasoning": "brief explanation"
        }
      `
    }]
  });

  results.push(JSON.parse(evaluation.content));
}

// Aggregate scores
const avgScore = results.reduce((sum, r) => sum + r.overall, 0) / results.length;
console.log(`Average quality: ${avgScore}/5`);
```

**Benefits**:
- ✅ **Fast**: 100 evals in ~5 minutes vs hours of human review
- ✅ **Consistent**: Same criteria applied every time
- ✅ **Detailed**: Get specific feedback on what needs improvement
- ✅ **Cheap**: $0.50-1.00 for 100 evaluations (Claude API)

**Judge LLM Options**:
- **Claude 3.5 Sonnet** - Best for nuanced evaluation ($3/M tokens)
- **GPT-4o** - Good general evaluator ($2.50/M tokens)
- **GPT-4o-mini** - Cheap for simple checks ($0.15/M tokens)
- **Local LLMs** - Free but less reliable (use for quick checks only)

**Example Output**:
```json
{
  "layer": "wine-expertise-v1",
  "testPrompts": 100,
  "judge": "claude-3.5-sonnet",
  "scores": {
    "accuracy": 4.6,
    "helpfulness": 4.8,
    "conciseness": 4.2,
    "style": 4.5,
    "overall": 4.5
  },
  "pass": true,
  "cost": "$0.85",
  "recommendations": [
    "Responses are accurate but sometimes overly verbose",
    "Consider training with more concise examples"
  ]
}
```

**3. Comparative Evaluation** (A vs B)

Compare two versions or base vs adapter:

```bash
# Compare base model vs adapter
./jtag layer/compare \
  --base "llama3.1:8b" \
  --adapter "wine-expertise-v1" \
  --test-prompts "./test-prompts.txt" \
  --judge "claude-3.5-sonnet"

# Output:
# Base model:  3.2/5 average
# With adapter: 4.6/5 average
# Improvement: +1.4 points (44% better) ✓
```

**4. Continuous Quality Monitoring**

For continuous learning, automatically evaluate each new version:

```bash
# Set up automated evaluation
./jtag training/continuous wine-expertise-v1 \
  --watch ./new-data/ \
  --auto-eval \
  --judge "claude-3.5-sonnet" \
  --min-quality 4.0 \
  --auto-deploy-if-better

# Behavior:
# 1. New data arrives → trigger training
# 2. Training completes → auto-evaluate with judge LLM
# 3. If quality >= 4.0 AND better than previous → auto-deploy
# 4. If quality < 4.0 → keep old version, alert user
```

**5. Multi-Judge Consensus** (High-stakes evaluation)

For critical layers, use multiple evaluators:

```bash
# Use PersonaUsers + cloud LLMs + humans as judges
./jtag layer/evaluate wine-expertise-v1 \
  --test-prompts "./test-prompts.txt" \
  --judges "helper-ai,teacher-ai,claude,gpt4,joel" \
  --require-consensus 0.8  # 80% agreement

# Evaluator types:
# - PersonaUsers (helper-ai, teacher-ai, etc.) - free, instant
# - Cloud LLMs (claude, gpt4) - paid, high quality
# - Humans (joel, sarah, etc.) - best quality, manual

# If judges disagree significantly, flag for human review
```

**Why Use PersonaUsers as Judges?**

PersonaUsers are perfect evaluators because:
- ✅ **Free**: No API costs
- ✅ **Fast**: Already running locally
- ✅ **Specialized**: Can assign domain experts (e.g., Helper AI reviews code layers)
- ✅ **Integrated**: Part of the same system
- ✅ **Diverse**: Multiple personas = multiple perspectives

**Example Evaluation Workflow**:

```typescript
// Send evaluation task to Helper AI persona
await Commands.execute('chat/send-message', {
  roomId: 'eval-room-uuid',
  userId: 'helper-ai-uuid',
  message: `
    @Helper AI: Please evaluate this wine expertise response.

    Prompt: "What is a natural wine?"
    Response: "Natural wine is produced with minimal intervention..."

    Rate 1-5 for:
    - Accuracy (are facts correct?)
    - Helpfulness (does it answer the question?)
    - Conciseness (appropriate detail level?)

    Respond with JSON only.
  `
});

// Helper AI responds after ~10 seconds
// {
//   "accuracy": 5,
//   "helpfulness": 4,
//   "conciseness": 5,
//   "reasoning": "Accurate definition, could elaborate on sulfites"
// }
```

**Distributed Evaluation Network**:

```bash
# Assign different personas to evaluate different aspects
./jtag layer/evaluate wine-expertise-v1 \
  --test-prompts "./test-prompts.txt" \
  --judges-by-criteria '{
    "accuracy": ["teacher-ai", "claude"],
    "helpfulness": ["helper-ai", "gpt4"],
    "style": ["joel", "general-ai"]
  }'

# Each persona/human evaluates their specialty
# Aggregate scores across all judges
# Fast + cheap (mostly free) + high quality
```

**Human-in-the-Loop**:

```bash
# For production layers, require human approval
./jtag layer/evaluate wine-expertise-v1 \
  --test-prompts "./test-prompts.txt" \
  --judges "helper-ai,teacher-ai,claude" \
  --require-human-approval \
  --human-reviewers "joel,sarah"

# Workflow:
# 1. PersonaUsers + LLMs auto-evaluate (fast)
# 2. If avg score >= 4.5, send to humans for final approval
# 3. Humans review 10 random examples (5 min)
# 4. If humans approve → deploy
# 5. If humans reject → analyze failures, retrain
```

---

## Production Training Pipeline

### Phase 1: Dataset Creation (Week 1)

```bash
# 1. Collect raw data
./jtag training/collect-data \
  --domain "wine" \
  --sources "wikipedia,kaggle,public-datasets" \
  --output "./raw/wine-raw.json"

# 2. Clean and format
./jtag training/format-qa \
  --input "./raw/wine-raw.json" \
  --output "./datasets/wine-expertise-qa.jsonl" \
  --validate

# 3. Generate synthetic examples (if needed)
./jtag training/generate-synthetic \
  --seed-dataset "./datasets/wine-expertise-qa.jsonl" \
  --model "gpt-4" \
  --target-size 100000 \
  --output "./datasets/wine-expertise-expanded.jsonl"

# 4. Split train/test
./jtag training/split-dataset \
  --input "./datasets/wine-expertise-expanded.jsonl" \
  --train 0.9 \
  --test 0.1
```

### Phase 2: Local Prototyping (Week 2)

```bash
# Train on 10% sample
./jtag training/train-lora \
  --base-model "llama3.1:8b" \
  --dataset "./datasets/wine-expertise-train-10pct.jsonl" \
  --output "./prototypes/wine-v0.1" \
  --epochs 1 \
  --quick

# Test prototype
./jtag training/evaluate \
  --adapter "./prototypes/wine-v0.1" \
  --test-dataset "./datasets/wine-expertise-test.jsonl"

# If quality good → proceed to full training
# If quality bad → fix dataset, retry
```

### Phase 3: Production Training (Week 3)

```bash
# Option A: Local (if GPU available)
./jtag training/train-lora \
  --base-model "llama3.1:8b" \
  --dataset "./datasets/wine-expertise-train.jsonl" \
  --output "./lora/wine-expertise-v1" \
  --rank 32 \
  --alpha 64 \
  --epochs 3 \
  --learning-rate 3e-4

# Option B: Fireworks AI (if no GPU)
./jtag training/train-cloud \
  --provider "fireworks" \
  --base-model "llama-v3p1-8b-instruct" \
  --dataset "./datasets/wine-expertise-train.jsonl" \
  --output "./lora/wine-expertise-v1" \
  --rank 32 \
  --alpha 64 \
  --epochs 3
```

### Phase 4: Validation & Publishing (Week 4)

```bash
# 1. Full evaluation
./jtag training/evaluate \
  --adapter "./lora/wine-expertise-v1" \
  --test-dataset "./datasets/wine-expertise-test.jsonl" \
  --human-eval \
  --num-samples 100

# 2. Package layer
./jtag layer/package wine-expertise-v1 \
  --adapter "./lora/wine-expertise-v1.safetensors" \
  --metadata "./wine-expertise-metadata.json" \
  --readme "./wine-expertise-README.md"

# 3. Test locally
./jtag layer/test wine-expertise-v1 \
  --prompts "./test-prompts.txt"

# 4. Publish to registry (when ready)
./jtag layer/publish wine-expertise-v1 \
  --registry "registry.continuum.ai" \
  --visibility "public"
```

---

## Cost Estimates (Per Layer) - VERIFIED 2025 PRICING

### Knowledge Layer (Wine Expertise - 100k examples)

**Local Training** (M1 Mac with MLX or RTX 3060 with Unsloth):
- Dataset generation: **$300** (Claude) or **$600** (GPT-4) for synthetic expansion
- Training: **$0** (hardware already owned)
- Time: 10-20 minutes (M1 with MLX) or 2-4 hours (RTX 3060 with Unsloth)
- **Total: $300-$600**
- Quality: Excellent

**Cloud Training** (Fireworks AI):
- Dataset generation: **$300-$600** (Claude/GPT-4 for synthetic expansion)
- Training: **$25** (100k examples × ~500 tokens/ex × 3 epochs = 50M tokens × $0.50/1M)
- **Total: $325-$625**
- Time: ~2 hours
- Quality: Excellent

### Personality Layer (Action Hero Style - 20k examples)

**Local Training** (M1 Mac with MLX):
- Dataset generation: **$150** (Claude) or **$300** (GPT-4)
- Training: **$0** (hardware already owned)
- Time: 5-10 minutes (M1 with MLX)
- **Total: $150-$300**
- Quality: Excellent

**Cloud Training** (Fireworks AI):
- Dataset generation: **$150-$300** (Claude/GPT-4)
- Training: **$5** (20k examples × ~500 tokens/ex × 2 epochs = 10M tokens × $0.50/1M)
- **Total: $155-$305**
- Time: ~30 minutes
- Quality: Excellent

### Hybrid Approach (Recommended)

**Prototype locally → Train production on cloud**:
- Prototyping: **$0** (local with MLX/Unsloth)
- Dataset generation: **$300-$600** (one-time cost)
- Production training: **$25-$30** (Fireworks AI)
- **Total: $325-$630 per layer**
- Time: 1 week (including dataset creation)
- Quality: Excellent

### Summary: First 5 Layers (2 Knowledge + 3 Personality)

**Recommended Approach** (Local training with cloud dataset generation):
- Dataset generation for all 5 layers: **~$1,500** (using Claude API)
- Training: **$0** (local with MLX on M1 Mac)
- **Total: ~$1,500 for all 5 production-quality layers**

**Alternative** (All cloud training):
- Dataset generation: **~$1,500**
- Training (Fireworks): **~$80** (2 × $25 + 3 × $5)
- **Total: ~$1,580 for all 5 production-quality layers**

---

## Recommended Strategy for Continuum: Multi-Provider Automation

**Philosophy**: Make it trivially easy to train with ANY provider, start/stop training, and continuously learn. Let users experiment with all options.

### Phase 7 (Now): Prove Architecture with Mocks
- No training yet
- Focus on GenomeDaemon + paging logic
- Mock adapters simulate real behavior

### Phase 8 (Next): Build Universal Training CLI

**Goal**: One command, multiple providers

```bash
# Universal training interface - provider is just a flag
./jtag training/train \
  --layer "action-hero-style-v1" \
  --dataset "./datasets/action-style.jsonl" \
  --provider "fireworks" \
  --rank 8 \
  --alpha 16 \
  --epochs 2

# Same command, different provider
./jtag training/train \
  --layer "action-hero-style-v1" \
  --dataset "./datasets/action-style.jsonl" \
  --provider "mlx" \
  --rank 8 \
  --alpha 16 \
  --epochs 2

# Or cloud provider
./jtag training/train \
  --layer "wine-expertise-v1" \
  --dataset "./datasets/wine-qa.jsonl" \
  --provider "openai" \
  --rank 32 \
  --alpha 64 \
  --epochs 3
```

**Supported Providers** (all pluggable):
- `mlx` - Apple Silicon (M1/M2/M3)
- `fireworks` - Fireworks AI
- `openai` - OpenAI fine-tuning API
- `anthropic` - Claude fine-tuning (when available)
- `replicate` - Replicate
- `together` - Together AI
- `modal` - Modal
- `local-gpu` - Raw PyTorch/HuggingFace (NVIDIA)

**Key Features**:
- Start/stop/resume training
- Cost estimation BEFORE starting
- Progress monitoring
- Auto-checkpointing every N steps
- Easy provider switching (same dataset, different provider)

### Phase 9: Continuous Learning Pipeline

**Goal**: Train continuously as new data comes in

```bash
# Start continuous learning daemon
./jtag training/continuous \
  --layer "wine-expertise-v1" \
  --watch-dataset "./datasets/wine-qa/" \
  --provider "fireworks" \
  --checkpoint-every 1000 \
  --auto-deploy

# Daemon behavior:
# 1. Watches dataset directory for new .jsonl files
# 2. Accumulates N new examples (e.g., 1000)
# 3. Triggers incremental training
# 4. Auto-deploys new version if quality improves
# 5. Keeps old version as fallback
```

**Continuous Learning Features**:
- Incremental training (don't retrain from scratch)
- Quality regression detection (auto-rollback if worse)
- Version management (v1.0.0 → v1.1.0 → v1.2.0)
- A/B testing (gradual rollout of new version)

### Phase 10: Multi-Provider Training Comparison

**Goal**: Train same layer on ALL providers, compare results

```bash
# Train on all providers simultaneously
./jtag training/benchmark \
  --layer "action-hero-style-v1" \
  --dataset "./datasets/action-style.jsonl" \
  --providers "fireworks,openai,mlx,together" \
  --rank 8 \
  --alpha 16

# Output:
# ┌───────────┬──────┬─────────┬─────────┬─────────┐
# │ Provider  │ Cost │ Time    │ Quality │ Winner  │
# ├───────────┼──────┼─────────┼─────────┼─────────┤
# │ mlx       │ $0   │ 8min    │ 4.2/5   │         │
# │ fireworks │ $5   │ 15min   │ 4.6/5   │ ✓ BEST  │
# │ openai    │ $12  │ 25min   │ 4.5/5   │         │
# │ together  │ $8   │ 20min   │ 4.3/5   │         │
# └───────────┴──────┴─────────┴─────────┴─────────┘
```

**Then let USERS decide** based on:
- Budget (free vs cheap vs expensive)
- Speed (minutes vs hours)
- Quality (measured by evals)
- Convenience (local vs cloud)

### Phase 11: Training Commands Suite

**Complete CLI for training lifecycle:**

```bash
# Dataset management
./jtag dataset/create wine-expertise --sources wikipedia,kaggle
./jtag dataset/validate wine-expertise --check-quality
./jtag dataset/split wine-expertise --train 0.9 --test 0.1

# Training
./jtag training/train wine-v1 --provider fireworks --dataset wine-train.jsonl
./jtag training/resume wine-v1 --checkpoint checkpoint-1000.ckpt
./jtag training/stop wine-v1 --save-checkpoint
./jtag training/status  # Show all active training jobs

# Evaluation
./jtag training/evaluate wine-v1 --test-dataset wine-test.jsonl
./jtag training/compare wine-v1 wine-v2 --ab-test

# Deployment
./jtag layer/publish wine-v1 --registry continuum.ai
./jtag layer/deploy wine-v1 --replace wine-v0.9 --gradual-rollout 0.1

# Continuous learning
./jtag training/continuous wine-v1 --watch ./new-data/ --auto-deploy
```

### Phase 12: Provider Plugins

**Make it trivial to add new providers:**

```typescript
// plugins/training-providers/together-ai.ts
export class TogetherAIProvider implements TrainingProvider {
  name = 'together';

  async train(config: TrainingConfig): Promise<TrainingResult> {
    // Call Together AI API
    const job = await together.fineTune.create({
      model: config.baseModel,
      dataset: config.dataset,
      hyperparameters: config.hyperparameters
    });

    return {
      jobId: job.id,
      cost: estimateCost(job),
      estimatedTime: job.eta
    };
  }

  async checkStatus(jobId: string): Promise<TrainingStatus> {
    // Poll job status
  }

  async download(jobId: string): Promise<Blob> {
    // Download trained adapter
  }
}
```

**Users can add their own providers** by dropping a file in `plugins/training-providers/`.

### Phase 13: Cost Tracking & Budgets

**Track spending across all providers:**

```bash
# Set monthly budget
./jtag training/set-budget --monthly 500

# View spending
./jtag training/costs --month 2025-11
# Output:
# ┌───────────┬────────┬──────────┬─────────┐
# │ Provider  │ Layers │ Cost     │ Status  │
# ├───────────┼────────┼──────────┼─────────┤
# │ fireworks │ 3      │ $45.20   │ ✓       │
# │ openai    │ 1      │ $120.00  │ ⚠ High  │
# │ mlx       │ 2      │ $0.00    │ ✓       │
# ├───────────┼────────┼──────────┼─────────┤
# │ TOTAL     │ 6      │ $165.20  │ 33% used│
# └───────────┴────────┴──────────┴─────────┘

# Stop training if budget exceeded
./jtag training/auto-stop --if-budget-exceeded
```

---

## Success Metrics

**Layer Quality**:
- Perplexity: <2.0 on test set
- Human rating: >4.0/5.0 average
- A/B test: >80% prefer adapter over base

**Training Efficiency**:
- Local: <4 hours per knowledge layer
- Cloud: <2 hours per knowledge layer
- Cost: <$50 per layer (cloud)

**Community Adoption**:
- Downloads: >100 per layer in first month
- Ratings: >4.5/5.0 average
- Derivatives: >5 personas using each layer

---

## Tools & Resources

**Local Training**:
- Unsloth: https://github.com/unslothai/unsloth
- Transformers: https://huggingface.co/transformers
- PEFT: https://github.com/huggingface/peft

**Cloud Training**:
- Fireworks AI: https://fireworks.ai/
- Replicate: https://replicate.com/
- Modal: https://modal.com/

**Dataset Creation**:
- GPT-4 API (OpenAI)
- Claude API (Anthropic)
- Public datasets (Kaggle, HuggingFace)

**Evaluation**:
- Perplexity evaluation scripts
- Human eval platforms (Scale AI, Surge)
- A/B testing framework

---

**Next Steps**: Implement training commands in Phase 8+

```bash
./jtag training/create-dataset --domain wine --target 100k
./jtag training/train-lora --local --dataset wine-qa.jsonl
./jtag training/evaluate --adapter wine-v1 --test-set wine-test.jsonl
./jtag layer/publish wine-expertise-v1 --registry continuum.ai
```
