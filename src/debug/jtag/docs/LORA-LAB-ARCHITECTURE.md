# LoRA Lab Architecture

**Status**: Design Draft
**Goal**: Mass-market LoRA layer economy - cross-platform

---

## Vision

A lab where users on **any hardware** can:
1. **Use** LoRA layers from others (download, plug in, go)
2. **Create** their own fine-tuned layers locally
3. **Share** layers with the community
4. **Compose** multiple layers for unique AI personalities

Think: **App Store for AI customizations** - but the "apps" are lightweight adapters that stack on base models.

### Platform Spectrum

```
┌─────────────────────────────────────────────────────────────────┐
│                     LORA LAB PLATFORMS                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  FLOOR (Minimum)              CEILING (Maximum)                  │
│  ─────────────────────────────────────────────────────────────  │
│                                                                  │
│  M1 Mac (8GB)                 RTX 5090 (32GB VRAM)              │
│  • Inference only             • Full training                    │
│  • Small adapters             • Large models (70B+)              │
│  • MLX backend                • CUDA/Unsloth                     │
│                                                                  │
│  M1 Pro/Max (16-32GB)         Docker/Ubuntu + CUDA              │
│  • Local training             • Production inference             │
│  • 3B-7B models               • Multi-GPU training               │
│  • MLX-LM, MLX-VLM            • Batch processing                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Key Insight**: Adapters are portable. Train on RTX 5090, deploy on M1 laptop.

---

## User Tiers

### Tier 1: Consumers (M1 8GB, entry laptops)
- Download and use adapters from others
- Swap between personalities/skills
- **Zero training required**
- Target: 80% of users

### Tier 2: Hobbyist Creators (M1 Pro 16GB+, gaming PCs)
- Train small adapters locally
- Share creations with community
- Experiment with composition
- Target: 15% of users

### Tier 3: Power Users (RTX 5090, cloud instances)
- Train large models (7B-70B)
- Create high-quality curated adapters
- Run production inference servers
- Train vision models
- Target: 5% of users

**The Economy**: Tier 3 creates → Tier 2 remixes → Tier 1 consumes

---

## User Scenarios

### Scenario A: Consumer (Inference Only)

```
User downloads adapter from Hub
         ↓
    Base Model (Llama 3.2 3B)
         +
    LoRA Adapter (2-50 MB)
         ↓
    Customized AI
```

**Requirements:**
- 8GB unified memory minimum
- No training needed
- ~30 second adapter load time
- Hot-swap between adapters

**Example workflow:**
```bash
# Browse available adapters
./jtag adapter/search --query="typescript expert"

# Download and activate
./jtag adapter/install --id="community/typescript-guru-v2"
./jtag adapter/activate --id="typescript-guru-v2"

# Use immediately
./jtag ai/generate --prompt="Review this TypeScript code..."
```

### Scenario B: Creator - Mac (Training + Inference)

```
User's training data (corrections, examples)
         ↓
    MLX LoRA Training (on M1)
         ↓
    Custom Adapter (.safetensors)
         ↓
    Publish to Hub (optional)
```

**Requirements:**
- 16GB+ unified memory recommended
- 10-60 minutes training time (500-2000 examples)
- Local dataset preparation tools

**Example workflow:**
```bash
# Prepare training data from chat corrections
./jtag training/prepare \
  --source="corrections" \
  --persona="helper-ai" \
  --output="./datasets/my-style.jsonl"

# Train locally on M1
./jtag training/start \
  --base="mlx-community/Llama-3.2-3B-Instruct-4bit" \
  --data="./datasets/my-style.jsonl" \
  --output="./adapters/my-style-v1.safetensors" \
  --epochs=3

# Test locally
./jtag adapter/activate --path="./adapters/my-style-v1.safetensors"

# Share with community (optional)
./jtag adapter/publish \
  --path="./adapters/my-style-v1.safetensors" \
  --name="my-writing-style" \
  --description="Trained on my chat corrections"
```

### Scenario C: Power User - Docker/CUDA (RTX 5090)

```
Large datasets + powerful GPU
         ↓
    Unsloth/PEFT Training (CUDA)
         ↓
    High-quality Adapter (.safetensors)
         ↓
    Publish to Hub (curated tier)
```

**Requirements:**
- RTX 5090 (32GB VRAM) or multi-GPU setup
- Docker with NVIDIA Container Toolkit
- Ubuntu 22.04+ or WSL2

**Example workflow:**
```bash
# Start training container
docker run --gpus all -v ./data:/data unsloth/unsloth:latest

# Train large model with Unsloth (4x faster than standard)
./jtag training/start \
  --backend="cuda" \
  --base="unsloth/Qwen2.5-7B-Instruct-bnb-4bit" \
  --data="./datasets/expert-corrections.jsonl" \
  --output="./adapters/expert-coder-v1.safetensors" \
  --epochs=5 \
  --lora-rank=64 \
  --batch-size=8

# Train vision model for UI critique
./jtag training/vision/start \
  --backend="cuda" \
  --base="Qwen/Qwen2.5-VL-7B-Instruct" \
  --data="./datasets/ui-critique.jsonl" \
  --output="./adapters/ui-expert-v1.safetensors" \
  --epochs=3

# Batch inference for production
./jtag inference/batch \
  --adapter="./adapters/expert-coder-v1.safetensors" \
  --input="./requests.jsonl" \
  --output="./responses.jsonl" \
  --batch-size=32
```

**Power user advantages:**
- 10-50x faster training than M1
- Larger models (7B-70B)
- Vision model training (Qwen2.5-VL 7B)
- Batch processing for production
- Multi-GPU parallelism

### Scenario D: Hybrid - API Key Acceleration

```
User has API key (Together, Fireworks, Modal)
         ↓
    Upload dataset (encrypted)
         ↓
    Cloud Training (minutes, not hours)
         ↓
    Download adapter locally
         ↓
    Run on any hardware
```

**For users who want:**
- Faster training without local GPU
- Access to larger models (70B+)
- Privacy-preserving (data deleted after training)
- Pay-per-use flexibility

**Example workflow:**
```bash
# Configure API key
./jtag config/set --key="TOGETHER_API_KEY" --value="tok_..."

# Offload training to cloud
./jtag training/offload \
  --provider="together" \
  --model="Qwen/Qwen2.5-72B-Instruct" \
  --data="./datasets/expert-level.jsonl" \
  --output="./adapters/cloud-trained.safetensors" \
  --privacy="delete-after-training"

# Download and use locally (runs on M1!)
./jtag adapter/activate --path="./adapters/cloud-trained.safetensors"
```

**Key insight**: Train on 72B model in cloud, deploy adapter on 3B model locally. The adapter captures the *knowledge*, base model provides the *capability*.

---

## Practical Model Tiers (All Free, All Local)

| Tier | Model | Memory | Training | Best For |
|------|-------|--------|----------|----------|
| **Tiny** | Moondream 1.6B | 4GB | 5-10 min | Vision Q&A, quick tests |
| **Small** | Llama 3.2 3B | 8GB | 15-30 min | Chat, coding, general |
| **Medium** | Qwen2.5 7B | 16GB | 30-60 min | Complex reasoning |
| **Vision** | Qwen2.5-VL 3B | 12GB | 20-40 min | UI, screenshots |
| **Large** | Llama 3.3 70B | 48GB+ | 2-4 hrs | Expert systems (RTX 5090) |

**Note:** All models run 100% locally. Training times for ~500 examples with LoRA rank 32.

**Free Tools Used:**
- MLX-LM (Mac) - Apple's open-source training framework
- Unsloth (CUDA) - 4x faster training, Apache 2.0 license
- Candle (Rust) - Our inference backend, MIT license
- HuggingFace Hub - Free model downloads

---

## Adapter Hub Structure

```
continuum-hub/
├── adapters/
│   ├── official/                    # Curated, tested
│   │   ├── typescript-expert-v3/
│   │   │   ├── adapter.safetensors  # 15 MB
│   │   │   ├── manifest.json        # Metadata
│   │   │   └── README.md            # Usage guide
│   │   ├── code-reviewer-v2/
│   │   └── ui-critique-v1/
│   │
│   ├── community/                   # User-contributed
│   │   ├── wine-sommelier/
│   │   ├── legal-assistant/
│   │   └── game-master-dnd/
│   │
│   └── personal/                    # Private (local only)
│       └── my-writing-style/
│
└── models/                          # Base models (optional cache)
    ├── llama-3.2-3b-instruct-4bit/
    └── qwen2.5-vl-3b-instruct-4bit/
```

### Manifest Schema

```json
{
  "id": "typescript-expert-v3",
  "name": "TypeScript Expert",
  "version": "3.0.0",
  "description": "Deep TypeScript knowledge with strict typing focus",
  "author": "continuum-team",
  "license": "MIT",

  "base_model": "mlx-community/Llama-3.2-3B-Instruct-4bit",
  "adapter_type": "lora",
  "lora_rank": 32,
  "lora_alpha": 64,

  "training": {
    "examples": 2500,
    "epochs": 3,
    "dataset_source": "typescript-corrections-2024"
  },

  "compatibility": {
    "min_memory_gb": 8,
    "platforms": ["darwin-arm64"],
    "inference_backends": ["mlx", "candle", "ollama"]
  },

  "metrics": {
    "downloads": 1234,
    "rating": 4.7,
    "verified": true
  },

  "files": {
    "adapter": "adapter.safetensors",
    "size_mb": 15.2
  }
}
```

---

## Adapter Composition

Stack multiple adapters for compound expertise:

```
Base Model: Llama 3.2 3B
    ↓
Layer 1: code-style-v2 (scale: 1.0)
    ↓
Layer 2: typescript-expert-v3 (scale: 0.8)
    ↓
Layer 3: my-personality (scale: 0.5)
    ↓
Result: AI with your style + TS expertise + code conventions
```

**Implementation:**
```bash
# Activate multiple adapters with scaling
./jtag adapter/compose \
  --adapters="code-style-v2:1.0,typescript-expert-v3:0.8,my-personality:0.5" \
  --save-as="my-composite-expert"
```

**Math:** `W' = W + scale₁(B₁A₁) + scale₂(B₂A₂) + scale₃(B₃A₃)`

---

## Local Training Pipeline (M1 Optimized)

### Step 1: Data Collection

```typescript
// Automatic collection from chat corrections
interface TrainingExample {
  instruction: string;  // What was asked
  input: string;        // Context provided
  output: string;       // Correct response
  source: 'correction' | 'thumbs_up' | 'import';
}
```

Sources:
- Chat corrections ("Actually, you should...")
- Thumbs up on good responses
- Imported datasets (JSONL)
- Failed tool calls with fixes

### Step 2: Training Script

```bash
#!/bin/bash
# scripts/train-lora-m1.sh

# Optimized for M1 unified memory
python -m mlx_lm.lora \
  --model "mlx-community/Llama-3.2-3B-Instruct-4bit" \
  --data "./datasets/training.jsonl" \
  --adapter-path "./adapters/output" \
  --lora-rank 32 \
  --lora-alpha 64 \
  --batch-size 2 \
  --iters 500 \
  --learning-rate 2e-4 \
  --warmup-ratio 0.1
```

### Step 3: Validation

```bash
# Quick quality check before deployment
./jtag training/validate \
  --adapter="./adapters/output" \
  --test-set="./datasets/test.jsonl" \
  --min-quality=0.8
```

### Step 4: Deployment

```bash
# Hot-swap into running inference
./jtag adapter/activate \
  --path="./adapters/output" \
  --replace="previous-adapter"
```

---

## Vision Model Workflow (UI/Design)

Vision models are **fully local and free** - same as text models.

### Training UI Expert (Any Platform)

**On Mac (M1/M2/M3):**
```bash
# 1. Collect UI screenshots with corrections
./jtag training/vision/collect \
  --source="screenshots" \
  --with-corrections

# 2. Train with MLX-VLM (free, local)
./jtag training/vision/start \
  --backend="mlx" \
  --base="mlx-community/Qwen2.5-VL-3B-Instruct-4bit" \
  --data="./datasets/ui-critique.jsonl" \
  --output="./adapters/ui-expert-v1.safetensors" \
  --epochs=3

# 3. Test on a screenshot
./jtag ai/vision/analyze \
  --image="./screenshot.png" \
  --adapter="ui-expert-v1" \
  --prompt="What usability issues do you see?"
```

**On CUDA (RTX 5090):**
```bash
# Same workflow, just different backend
./jtag training/vision/start \
  --backend="cuda" \
  --base="Qwen/Qwen2.5-VL-7B-Instruct" \
  --data="./datasets/ui-critique.jsonl" \
  --output="./adapters/ui-expert-v1.safetensors" \
  --epochs=5 \
  --batch-size=8
```

### Example Training Data Format

```jsonl
{"image": "ui-001.png", "conversations": [{"from": "human", "value": "What's wrong with this UI?"}, {"from": "assistant", "value": "The CTA button has low contrast..."}]}
{"image": "ui-002.png", "conversations": [{"from": "human", "value": "Rate this layout"}, {"from": "assistant", "value": "3/5. The visual hierarchy is unclear..."}]}
```

### Vision Adapter Examples

| Adapter | Domain | Training Data | Use Case |
|---------|--------|---------------|----------|
| ui-critique | UI/UX | 1000 annotated screenshots | Design feedback |
| code-screenshot | Code | IDE screenshots + explanations | Visual code review |
| diagram-reader | Architecture | System diagrams + descriptions | Doc generation |
| accessibility-check | A11y | WCAG violation examples | Compliance audit |

---

## Multi-Modal Outputs

LoRA adapters aren't just for text - they work across **all generative modalities**.

### Modality Spectrum

```
┌─────────────────────────────────────────────────────────────────┐
│                     OUTPUT MODALITIES                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  TEXT (Current Focus)                                           │
│  • Chat, code, writing, analysis                                 │
│  • Llama 3.2, Qwen2.5                                           │
│  • Training: 30 min on M1                                        │
│                                                                  │
│  VISION INPUT (Phase 6)                                          │
│  • UI analysis, screenshot understanding                         │
│  • Moondream, Qwen2.5-VL                                        │
│  • Training: 1 hr on M1, 15 min on CUDA                         │
│                                                                  │
│  IMAGE OUTPUT (Phase 9)                                          │
│  • Custom art styles, logo generation                            │
│  • SDXL, Flux, custom diffusion                                  │
│  • Training: CUDA required (16GB+ VRAM)                          │
│                                                                  │
│  AUDIO OUTPUT (Phase 10)                                         │
│  • Voice cloning, TTS styles, music                              │
│  • XTTS, MusicGen, Bark                                         │
│  • Training: CUDA recommended                                    │
│                                                                  │
│  VIDEO (Future)                                                  │
│  • Style transfer, motion synthesis                              │
│  • SVD, CogVideo, Sora-like                                     │
│  • Training: Multi-GPU required                                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Image Generation Adapters

For personas that want to CREATE images, not just understand them:

```bash
# Train custom art style on CUDA
./jtag training/diffusion/start \
  --backend="cuda" \
  --base="stabilityai/stable-diffusion-xl-base-1.0" \
  --data="./datasets/my-art-style/" \
  --output="./adapters/my-art-lora.safetensors" \
  --steps=2000

# Generate with style
./jtag ai/image/generate \
  --adapter="my-art-lora" \
  --prompt="A cyberpunk city at sunset" \
  --output="./generated.png"
```

### Audio Adapters

Voice cloning, custom TTS, music generation:

```bash
# Clone voice (requires audio samples)
./jtag training/audio/start \
  --backend="cuda" \
  --base="coqui/XTTS-v2" \
  --data="./voice-samples/" \
  --output="./adapters/my-voice.safetensors"

# Generate speech with cloned voice
./jtag ai/audio/speak \
  --adapter="my-voice" \
  --text="Hello, this is my cloned voice" \
  --output="./speech.wav"
```

### Community Specialization

Different users contribute different modalities:

| Hardware | Best For | Community Role |
|----------|----------|----------------|
| M1 8GB | Text inference, small adapters | Consumers, light creators |
| M1 Pro 32GB | Text training, vision inference | Text specialists |
| RTX 3080 16GB | Small diffusion, vision training | Image hobbyists |
| RTX 5090 32GB | All modalities, large models | Full creators |
| Multi-GPU | Video, large diffusion | Production studios |

**The Exchange**: Someone with RTX 5090 trains diffusion adapters → shares with M1 users who can only run inference → M1 users train text adapters → share back.

### Impatient Path (Cloud On-Demand)

Don't have the hardware? Don't want to wait? Pay for speed:

```bash
# Train diffusion adapter in cloud (minutes instead of hours)
./jtag training/diffusion/start \
  --offload=true \
  --base="stabilityai/stable-diffusion-xl-base-1.0" \
  --data="./my-art-style/" \
  --output="./adapters/my-art-lora.safetensors"

# Download and run locally (inference works on M1!)
./jtag ai/image/generate \
  --adapter="my-art-lora" \
  --prompt="A landscape in my style"
```

**The trade-off:**
- **Local (free, slow)**: Train overnight, own your compute
- **Cloud (pay, fast)**: Train in minutes, pay per use

Both produce identical adapters. Choose based on patience.

---

## Cross-Domain Adapters

The adapter system works for **any domain** - not just code. Train on your expertise, share with others.

### Example Domains

| Domain | Adapter Ideas | Training Source |
|--------|--------------|-----------------|
| **Code** | TypeScript expert, code reviewer, debugging specialist | Corrections, PRs, reviews |
| **Design** | UI critique, color theory, accessibility | Annotated screenshots |
| **Writing** | Technical docs, creative writing, tone matching | Edited drafts |
| **Legal** | Contract review, clause analysis | Expert annotations |
| **Medical** | Symptom triage, drug interactions | Clinical guidelines |
| **Gaming** | D&D dungeon master, game balance | Session transcripts |
| **Music** | Chord progression, lyric writing | Song analyses |
| **Finance** | Stock analysis, risk assessment | Market commentary |
| **Language** | Translation style, dialect matching | Parallel texts |
| **Education** | Tutoring style, explanation clarity | Student interactions |

### Domain Specialization Pattern

```bash
# 1. Collect domain-specific corrections
./jtag training/prepare \
  --source="corrections" \
  --domain="legal" \
  --output="./datasets/legal-review.jsonl"

# 2. Train specialist adapter
./jtag training/start \
  --data="./datasets/legal-review.jsonl" \
  --output="./adapters/legal-reviewer-v1.safetensors" \
  --epochs=3

# 3. Compose with base skills
./jtag adapter/compose \
  --adapters="legal-reviewer-v1:1.0,writing-clarity:0.5" \
  --save-as="legal-writer"
```

### Community Growth Model

```
┌─────────────────────────────────────────────────────────────────┐
│                     ADAPTER ECOSYSTEM                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Week 1: Early adopters train personal style adapters           │
│          └── 10 adapters, mostly code-focused                   │
│                                                                  │
│  Month 1: Power users create domain specialists                 │
│           └── 100 adapters, diverse domains emerging            │
│                                                                  │
│  Month 6: Community remixes and compositions                    │
│           └── 1000 adapters, composition patterns emerge        │
│                                                                  │
│  Year 1: Ecosystem of specialized stacks                        │
│          └── 10000+ adapters, emergent expertise combos         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Key Insight**: The value isn't in any single adapter - it's in the combinations. A "legal-typescript-concise" stack is more valuable than any piece alone.

---

## Cloud Offload (Optional)

**Not required.** Only for users who want to trade money for time.

```bash
# Optional: Configure API key for acceleration
./jtag config/set --key="TOGETHER_API_KEY" --value="tok_..."

# Training automatically uses cloud when --offload is set
./jtag training/start \
  --data="./datasets/training.jsonl" \
  --output="./adapters/my-adapter.safetensors" \
  --offload=true  # Optional: uses cloud if API key exists
```

Supported providers (all pay-per-use):
- Together AI - Best for Llama/Qwen models
- Fireworks AI - Fast inference focus
- Modal - Flexible GPU rentals
- RunPod - Cheapest per-hour

**Without API key**: Everything works locally. Zero cost.

---

## Economy Model

### Core Principle: Free First, Always

**Everything essential is free and local.** No accounts, no subscriptions, no usage limits.

### Free Forever (Default)
- Download unlimited adapters from community
- Train locally on your hardware (any backend)
- Share unlimited adapters with community
- Compose and remix adapters freely
- Full source code access (open source)

### Optional Acceleration (API Keys)
For users who WANT to pay for speed:
- Cloud training offload (Together, Fireworks, Modal)
- Train 70B+ models without local GPU
- 10-100x faster than local training
- Data privacy options (delete after training)

**How it works:**
```bash
# Add optional API key for acceleration
./jtag config/set --key="TOGETHER_API_KEY" --value="tok_..."

# Now training can optionally use cloud
./jtag training/start --offload=true  # Uses cloud if API key set
./jtag training/start                  # Always local, always free
```

### Adapter Trading
The real economy is **adapters**, not subscriptions:
- Power users create high-quality adapters
- Community remixes and improves
- Everyone benefits from collective learning
- No paywalls, just reputation/ratings

---

## Commands Summary

```bash
# Discovery
./jtag adapter/search --query="python expert"
./jtag adapter/list --filter="vision"
./jtag adapter/info --id="typescript-expert-v3"

# Installation
./jtag adapter/install --id="community/wine-sommelier"
./jtag adapter/uninstall --id="wine-sommelier"
./jtag adapter/update --id="typescript-expert-v3"

# Activation
./jtag adapter/activate --id="typescript-expert-v3"
./jtag adapter/deactivate
./jtag adapter/compose --adapters="a:1.0,b:0.5"

# Training
./jtag training/prepare --source="corrections"
./jtag training/start --data="./data.jsonl"
./jtag training/status
./jtag training/validate --adapter="./output"

# Publishing
./jtag adapter/publish --path="./adapter" --name="my-adapter"
./jtag adapter/unpublish --id="my-adapter"
```

---

## Success Metrics

1. **Accessibility**: 80% of M1 Mac users can train a LoRA in < 30 min
2. **Adapter Size**: Average adapter < 50 MB (easy to share)
3. **Quality**: Community adapters rated 4+ stars on average
4. **Composition**: Users create 3+ adapter combinations on average
5. **Economy**: Active adapter sharing/trading ecosystem

---

## Implementation Phases

### Phase 1: Rust Inference Backend (DONE)
- [x] Candle gRPC worker with Metal support
- [x] Model hot-swap via RPC commands
- [x] GPU memory allocator with smart eviction
- [x] Proto schema for model/adapter management

### Phase 2: LoRA Weight Loading (IN PROGRESS)
- [x] Proto schema for adapter load/unload
- [ ] Safetensor parsing in Rust
- [ ] LoRA weight merging with base model
- [ ] Hot-swap adapters without model reload

### Phase 3: Training Commands
- [ ] `training/prepare` - Collect corrections → JSONL
- [ ] `training/start` - Launch MLX/Unsloth training
- [ ] `training/status` - Monitor training progress
- [ ] `training/validate` - Quality check adapter

### Phase 4: Adapter Management
- [ ] `adapter/install` - Download from hub
- [ ] `adapter/activate` - Load into inference worker
- [ ] `adapter/deactivate` - Unload adapter
- [ ] `adapter/list` - Show installed adapters

### Phase 5: Composition
- [ ] Multi-adapter loading in Rust worker
- [ ] Scale factor support per adapter
- [ ] `adapter/compose` - Merge adapters
- [ ] Composite adapter saving

### Phase 6: Vision Models
- [ ] Moondream integration in Candle
- [ ] Image encoding in gRPC proto
- [ ] `ai/vision/*` commands
- [ ] MLX-VLM training wrapper

### Phase 7: Hub & Sharing
- [ ] Adapter manifest schema validation
- [ ] `adapter/publish` command
- [ ] Community hub API
- [ ] Rating/discovery system

### Phase 8: Cloud Offload (Optional)
- [ ] Together/Fireworks API integration
- [ ] `training/offload` command
- [ ] Automatic adapter download

### Phase 9: Diffusion/Image Generation
- [ ] SDXL LoRA training wrapper
- [ ] `training/diffusion/start` command
- [ ] `ai/image/generate` command
- [ ] Diffusion adapter hub integration

### Phase 10: Audio Generation
- [ ] XTTS/voice cloning integration
- [ ] `training/audio/start` command
- [ ] `ai/audio/speak` command
- [ ] Audio adapter formats

### Phase 11: Video (Future)
- [ ] Research SVD/CogVideo integration
- [ ] Video LoRA training (multi-GPU)
- [ ] `ai/video/generate` command

---

## References

- [MLX-LM LoRA](https://github.com/ml-explore/mlx-examples/tree/main/lora) - Apple's LoRA training
- [MLX-VLM](https://github.com/Blaizzy/mlx-vlm) - Vision model training on Mac
- [CONTINUOUS-LEARNING-RUNTIME.md](CONTINUOUS-LEARNING-RUNTIME.md) - Runtime architecture
- [LORA-TRAINING-STRATEGY.md](LORA-TRAINING-STRATEGY.md) - Training approaches
