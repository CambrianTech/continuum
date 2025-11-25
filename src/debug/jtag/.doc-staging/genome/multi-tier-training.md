# Multi-Tier LoRA Training Strategy (Proof of Concept)

**Philosophy**: Support everything from tiny local models to SOTA cloud APIs, with efficient routing based on use case.

---

## Tier 1: Local Models (FREE, Private)

### Small Local (Testing & Fast Iteration)
| Model | Size | HF ID | Memory | Speed | Use Case |
|-------|------|-------|--------|-------|----------|
| **SmolLM2-135M** | 135M | `HuggingFaceTB/SmolLM2-135M-Instruct` | 2GB | 30s/100ex | Unit tests, POC |
| **TinyLlama-1.1B** | 1.1B | `TinyLlama/TinyLlama-1.1B-Chat-v1.0` | 4GB | 2min/100ex | Fast experiments |

**Status**: ✅ Already downloaded

### Decent Local (Production)
| Model | Size | HF ID | Memory | Speed | Use Case |
|-------|------|-------|--------|-------|----------|
| **Llama-3.2-1B** | 1.2B | `meta-llama/Llama-3.2-1B` | 6GB | 3min/100ex | Lightweight personas |
| **Llama-3.2-3B** | 3.2B | `meta-llama/Llama-3.2-3B` | 10GB | 7min/100ex | Balanced personas |
| **Phi-3-mini** | 3.8B | `microsoft/Phi-3-mini-4k-instruct` | 11GB | 8min/100ex | Coding/reasoning |
| **Qwen2.5-3B** | 3B | `Qwen/Qwen2.5-3B-Instruct` | 10GB | 6min/100ex | Multilingual |

**Status**: ❌ Need to download (auto-download on first use)

**Training Method**: PEFTLoRAAdapter (PyTorch + PEFT)
- **Cost**: $0 (free, electricity only)
- **Privacy**: 100% local, data never leaves machine
- **Deployment**: PEFT → GGUF → Ollama

---

## Tier 2: Remote Fast/Cheap (API, Cost-Effective)

### Fast Inference APIs (Good for serving, not training)
| Provider | Model | Cost | Speed | Use Case |
|----------|-------|------|-------|----------|
| **Fireworks** | Llama-3.1-8B | $0.20/1M tok | 200ms | Fast inference |
| **Together** | Llama-3-8B | $0.20/1M tok | 150ms | Fast inference |
| **Groq** | Llama-3.1-8B | $0.05/1M tok | 50ms | Fastest inference |

**Note**: These are primarily inference APIs - most don't offer fine-tuning. Use local training + deploy to these for inference.

### Cheap Training APIs
| Provider | Model | Training Cost | Status | Adapter |
|----------|-------|--------------|--------|---------|
| **DeepSeek** | DeepSeek-Chat | $0.55/1M in, $2.19/1M out | ✅ Implemented | DeepSeekLoRAAdapter |
| **OpenAI** | GPT-3.5 | $8/1M tokens | ✅ Implemented | OpenAILoRAAdapter |
| **Together** | Various 7B+ | ~$1/1M tokens | ❌ Need adapter | TogetherLoRAAdapter |

**Status**:
- DeepSeekLoRAAdapter: ✅ Code exists (needs API key)
- OpenAILoRAAdapter: ✅ Code exists (needs API key)
- TogetherLoRAAdapter: ❌ Need to create

---

## Tier 3: SOTA (Best Quality, Expensive)

### State-of-the-Art Models
| Provider | Model | Training Cost | Quality | Status |
|----------|-------|--------------|---------|--------|
| **OpenAI** | GPT-4o-mini | $3/1M in, $12/1M out | Excellent | ✅ Implemented |
| **OpenAI** | GPT-4o | $25/1M in, $100/1M out | SOTA | ✅ Implemented |
| **Anthropic** | Claude-3.5-Sonnet | ~$15/1M tokens | SOTA | ✅ Implemented |
| **Anthropic** | Claude-3-Opus | ~$75/1M tokens | SOTA | ✅ Implemented |

**Status**:
- OpenAILoRAAdapter: ✅ Code exists (needs API key for GPT-4)
- AnthropicLoRAAdapter: ✅ Code exists (needs API key)

**Use Case**: Production personas requiring highest quality, when cost isn't primary concern

---

## Proof of Concept Plan

### Phase 1: Validate Local Training (Priority 1)
**Goal**: Prove PEFT training works end-to-end

1. **Test with SmolLM2-135M** (smallest, fastest)
   - Already downloaded
   - 5 examples, 1 epoch
   - Expected time: ~30 seconds
   - Validates: Python env, PEFT, training loop

2. **Test with TinyLlama-1.1B** (realistic size)
   - Already downloaded
   - 10 examples, 2 epochs
   - Expected time: ~2 minutes
   - Validates: LoRA works on real model

3. **Test with Llama-3.2-1B** (production-ready)
   - Auto-download (~1.5 GB)
   - 50 examples, 3 epochs
   - Expected time: ~5 minutes
   - Validates: Production pipeline

**Deliverable**: Working local training → LoRA adapter files

---

### Phase 2: Add GGUF Conversion (Priority 2)
**Goal**: Deploy trained adapters to Ollama

1. **Create conversion script**
   ```bash
   python3 scripts/convert-peft-to-gguf.py \
     --adapter-path .continuum/genome/adapters/tinyllama-conversational-123456 \
     --base-model TinyLlama/TinyLlama-1.1B-Chat-v1.0 \
     --output adapter.gguf
   ```

2. **Create Ollama deployment**
   ```bash
   # Generate Modelfile
   cat > Modelfile <<EOF
   FROM tinyllama:1.1b
   ADAPTER ./adapter.gguf
   EOF

   # Create model
   ollama create persona-helper:latest -f Modelfile
   ```

3. **Test inference**
   ```bash
   ollama run persona-helper:latest "Test the trained persona"
   ```

**Deliverable**: End-to-end pipeline (Train → Convert → Deploy → Inference)

---

### Phase 3: Add Remote Training (Priority 3)
**Goal**: Support cloud APIs for SOTA quality

1. **Test DeepSeek** (cheapest cloud)
   - Set API key: `DEEPSEEK_API_KEY`
   - Train with 50 examples
   - Cost: ~$0.05
   - Validates: Cloud training works

2. **Add Fireworks/Together adapters** (if needed)
   - Clone DeepSeekLoRAAdapter structure
   - Update API endpoints
   - Test with small dataset

3. **Test OpenAI GPT-4o-mini** (SOTA)
   - Set API key: `OPENAI_API_KEY`
   - Train with 50 examples
   - Cost: ~$0.50
   - Validates: Premium training works

**Deliverable**: Multi-tier adapter routing (local → cheap cloud → SOTA)

---

### Phase 4: Adapter Routing Logic (Priority 4)
**Goal**: Automatically choose best adapter for use case

```typescript
interface TrainingRequest {
  dataset: TrainingDataset;
  quality: 'fast' | 'balanced' | 'best';
  budget: 'free' | 'cheap' | 'unlimited';
  privacy: 'required' | 'preferred' | 'optional';
}

function selectAdapter(request: TrainingRequest): LoRAAdapter {
  // Privacy required → Local only
  if (request.privacy === 'required') {
    return selectLocalAdapter(request.quality);
  }

  // Budget = free → Local only
  if (request.budget === 'free') {
    return new PEFTLoRAAdapter(); // Best local
  }

  // Budget = cheap + Quality = fast → DeepSeek
  if (request.budget === 'cheap' && request.quality === 'fast') {
    return new DeepSeekLoRAAdapter(); // $0.55/1M
  }

  // Quality = best + Budget = unlimited → SOTA
  if (request.quality === 'best' && request.budget === 'unlimited') {
    return new OpenAILoRAAdapter(); // GPT-4o
  }

  // Default: Local PEFT (free, private, good quality)
  return new PEFTLoRAAdapter();
}
```

**Deliverable**: Smart adapter selection based on requirements

---

## Model Matrix (Complete Coverage)

### Local Training (PEFT)
| Category | Models | Status | Download Size |
|----------|--------|--------|---------------|
| **Tiny** | SmolLM2-135M, TinyLlama-1.1B | ✅ Downloaded | 0 GB |
| **Small** | Llama-3.2-1B, Phi-3-mini | ❌ Need download | 3.5 GB total |
| **Medium** | Llama-3.2-3B, Qwen2.5-3B | ❌ Need download | 6 GB total |
| **Large** | Llama-3.1-8B, Mistral-7B | ❌ Optional | 15 GB total |

### Cloud Training (API)
| Tier | Provider | Models | Cost/1M | Status |
|------|----------|--------|---------|--------|
| **Cheap** | DeepSeek | DeepSeek-Chat | $0.55 | ✅ Ready |
| **Mid** | OpenAI | GPT-3.5, GPT-4o-mini | $3-8 | ✅ Ready |
| **SOTA** | OpenAI | GPT-4o | $25 | ✅ Ready |
| **SOTA** | Anthropic | Claude-3.5 | $15 | ✅ Ready |

### Missing Adapters (Optional)
| Provider | Value | Priority |
|----------|-------|----------|
| **Fireworks** | Fast inference | Low (inference only) |
| **Together** | Cheap training | Medium |
| **Groq** | Fastest inference | Low (inference only) |
| **Replicate** | Easy deployment | Low |

---

## Implementation Checklist

### ✅ Already Complete
- [x] PEFTLoRAAdapter (local training)
- [x] DeepSeekLoRAAdapter (cheapest cloud)
- [x] OpenAILoRAAdapter (SOTA cloud)
- [x] AnthropicLoRAAdapter (SOTA cloud)
- [x] Python environment (PyTorch, PEFT, Transformers)
- [x] Local models downloaded (SmolLM2, TinyLlama)
- [x] Genome paging system (memory management)

### ⏳ In Progress
- [ ] Test PEFT with SmolLM2/TinyLlama (Phase 1)
- [ ] GGUF conversion script (Phase 2)
- [ ] Ollama deployment automation (Phase 2)

### 📋 TODO
- [ ] Download production models (Llama-3.2-1B, Llama-3.2-3B)
- [ ] Test DeepSeek API training (Phase 3)
- [ ] Add adapter routing logic (Phase 4)
- [ ] (Optional) Add Together/Fireworks adapters

---

## Success Metrics

### Phase 1 Success
- ✅ SmolLM2 training completes in <1 minute
- ✅ TinyLlama training completes in <3 minutes
- ✅ LoRA adapter files created in `.continuum/genome/adapters/`
- ✅ No Python errors

### Phase 2 Success
- ✅ PEFT adapter converts to GGUF format
- ✅ Ollama loads GGUF adapter
- ✅ Inference works with trained adapter
- ✅ Measurable quality improvement vs base model

### Phase 3 Success
- ✅ DeepSeek API training works
- ✅ Cost < $0.10 for test dataset
- ✅ Quality comparable to local 3B model

### Phase 4 Success
- ✅ Adapter routing selects appropriate model
- ✅ Performance benchmarks documented
- ✅ Cost analysis documented

---

## Next Steps

**Immediate**: Test Phase 1 with SmolLM2-135M (30 seconds to validate setup)

**Command to run**:
```bash
npx tsx system/genome/fine-tuning/server/adapters/test-peft.ts
```

Ready to proceed?
