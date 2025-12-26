# VRAM Calculator Integration Plan

**Goal**: Integrate apxml.com-style VRAM calculator into Continuum's content tab system for LoRA training planning.

**Inspired by**: https://apxml.com/tools/vram-calculator

---

## Architecture: VSCode-Style Content Tabs

### Current System (from ContentTypes.ts)
```typescript
interface ContentInfo {
  id: string;
  name: string;
  type: 'room' | 'user_chat' | 'system';  // ← ADD 'tool' type
  path: string;                            // ← e.g., '/tools/vram-calculator'
  displayName: string;
  description?: string;
  isActive: boolean;
}
```

### URL Routing Pattern
- **Chat rooms**: `/rooms/general`, `/rooms/academy`
- **User chats**: `/users/{userId}/chat`
- **Tools**: `/tools/vram-calculator` ← NEW
- **Diagnostics**: `/diagnostics` (future)
- **Training monitor**: `/training/{sessionId}` (future)

### Tab Behavior (VSCode-style)
- **Multiple tabs** can be open simultaneously
- **Active tab** shows in main content area
- **URL rewrites** on tab switch (`/rooms/general` → `/tools/vram-calculator`)
- **Tab persistence** across sessions

---

## VRAM Calculator Features (from apxml.com)

### Input Parameters
1. **Model Selection**
   - Dropdown with 100+ models (Llama, Qwen, Mistral, DeepSeek, Gemma, Phi, etc.)
   - Auto-populate: parameter count, architecture, context length
   - Source: Our `POPULAR-MODELS-BY-PROVIDER.md` + provider BaseConfigs

2. **Training Configuration**
   - LoRA rank (r): 1-128 (default: 16)
   - Batch size: 1-128 (default: 4)
   - Gradient accumulation steps: 1-32 (default: 1)
   - Sequence length: 512-131072 (default: 2048)
   - Precision: 4-bit, 8-bit, 16-bit, 32-bit (default: 4-bit)

3. **Hardware Selection**
   - **Apple Silicon**: M1/M2/M3 (8GB, 16GB, 24GB, 32GB, 64GB, 96GB, 128GB)
   - **NVIDIA Consumer**: RTX 3060 (12GB), 3090 (24GB), 4060 Ti (16GB), 4090 (24GB)
   - **NVIDIA Pro**: A100 (40GB/80GB), H100 (80GB)
   - **AMD**: Radeon VII (16GB), MI210 (64GB), MI300X (192GB)
   - **Custom**: Manual VRAM entry

4. **Optimization Toggles**
   - Flash Attention (45% VRAM savings)
   - Gradient Checkpointing (70% VRAM savings)
   - 8-bit Optimizer (75% VRAM savings)
   - CPU Offloading (dynamic VRAM savings)
   - LoRA+ (separate learning rates, minimal VRAM impact)

### Output Display

**Memory Breakdown (Pie Chart)**
```
Total: 5.75 GB
├─ Base Model: 3.00 GB (52.1%)  ← Model weights in selected precision
├─ Activations: 1.41 GB (24.5%)  ← Forward pass intermediate results
├─ Framework: 1.31 GB (22.7%)    ← PyTorch/framework overhead
└─ LoRA: 0.04 GB (0.7%)          ← LoRA adapter weights (tiny!)
```

**Performance Metrics**
- **Training speed**: ~18 tok/sec (for DeepSeek-R1 1.5B on M2 Pro)
- **Estimated time**: Calculate based on dataset size + tok/sec
- **Cost estimate**: For cloud providers ($/hour * estimated hours)

**Feasibility Check**
- ✅ **Fits in VRAM** (5.75 GB < 16 GB available)
- ⚠️ **Tight fit** (90%+ VRAM utilization, may need tweaks)
- ❌ **Won't fit** (exceeds available VRAM, suggest optimizations)

**Recommendations**
- Reduce batch size to X
- Enable gradient checkpointing
- Use 4-bit quantization instead of 8-bit
- Switch to smaller model variant
- Try CPU offloading

---

## Implementation Plan

### Phase 1: Calculator Widget (UI Only)

**File Structure**:
```
widgets/
└── tools/
    └── vram-calculator/
        ├── shared/
        │   ├── VramCalculatorTypes.ts       # Calculator interfaces
        │   └── VramCalculatorLogic.ts       # Memory calculation formulas
        ├── browser/
        │   └── VramCalculatorWidget.ts      # Main widget
        └── public/
            ├── vram-calculator.css          # Calculator styling
            └── vram-calculator.html         # Widget template
```

**Key Classes**:
```typescript
// VramCalculatorTypes.ts
interface VramCalculatorConfig {
  model: ModelSelection;
  training: TrainingConfig;
  hardware: HardwareSelection;
  optimizations: OptimizationFlags;
}

interface MemoryEstimate {
  baseModel: number;      // MB
  activations: number;    // MB
  framework: number;      // MB
  lora: number;           // MB
  total: number;          // MB
  breakdown: MemoryBreakdown;
}

interface PerformanceEstimate {
  tokensPerSecond: number;
  estimatedTimeSeconds: number;
  costEstimate?: number;  // USD
}

// VramCalculatorLogic.ts
class VramCalculator {
  calculateMemory(config: VramCalculatorConfig): MemoryEstimate;
  estimatePerformance(config: VramCalculatorConfig): PerformanceEstimate;
  checkFeasibility(estimate: MemoryEstimate, hardware: HardwareSelection): FeasibilityResult;
  suggestOptimizations(estimate: MemoryEstimate, hardware: HardwareSelection): Recommendation[];
}
```

**Memory Calculation Formulas** (from LOCAL-TRAINING-PHASE2.md):
```typescript
baseModelMemory = (parameterCount * bytesPerParam) / (1024^3)
loraMemory = (loraRank * 2 * sumOfLayerDimensions * bytesPerParam) / (1024^3)
optimizerMemory = (numTrainableParams * 8) / (1024^3)  // Adam optimizer
gradientsMemory = (numTrainableParams * bytesPerParam) / (1024^3)
activationsMemory = (batchSize * seqLength * hiddenDim * numLayers * bytesPerParam) / (1024^3)

totalMemory = baseModelMemory + loraMemory + optimizerMemory + gradientsMemory + activationsMemory
```

**Optimization Multipliers**:
```typescript
if (flashAttention) activationsMemory *= 0.55;      // 45% savings
if (gradientCheckpointing) activationsMemory *= 0.30;  // 70% savings
if (optimizer8bit) optimizerMemory *= 0.25;         // 75% savings
```

### Phase 2: Integration with Provider System

**Connect to Provider Adapters**:
```typescript
// Fetch models from all providers
const allModels = await Promise.all([
  openAIConfig.getAvailableModels(),
  deepseekConfig.getAvailableModels(),
  fireworksConfig.getAvailableModels(),
  // ... etc
]);

// Filter for fine-tuning capable models
const fineTuneModels = allModels
  .flat()
  .filter(m => m.capabilities?.includes('fine-tuning'));

// Populate calculator dropdown
populateModelSelector(fineTuneModels);
```

**Cost Estimation**:
```typescript
// Get provider pricing from BaseConfigs
const costPerHour = getProviderCost(selectedProvider, selectedModel);
const estimatedHours = totalTokens / tokensPerSecond / 3600;
const totalCost = costPerHour * estimatedHours;
```

### Phase 3: Content Routing Integration

**Update ContentTypes.ts**:
```typescript
interface ContentInfo {
  type: 'room' | 'user_chat' | 'system' | 'tool';  // ← ADD 'tool'
  path: string;  // '/tools/vram-calculator'
}
```

**Register Tool Content**:
```typescript
// In ContentInfoManager
async getContentByPath(path: string): Promise<ContentInfo | null> {
  const [, pathType, contentId] = path.split('/');

  if (pathType === 'chat') {
    return await this.getChatContentInfo(contentId);
  }

  if (pathType === 'tools') {
    return await this.getToolContentInfo(contentId);  // ← NEW
  }

  return null;
}

private async getToolContentInfo(toolId: string): Promise<ContentInfo> {
  const toolConfigs = {
    'vram-calculator': {
      name: 'vram-calculator',
      displayName: 'VRAM Calculator',
      description: 'Estimate memory requirements for LoRA fine-tuning',
      widgetType: 'vram-calculator-widget'
    }
  };

  const config = toolConfigs[toolId];
  return {
    id: toolId,
    name: config.name,
    type: 'tool',
    path: `/tools/${toolId}`,
    displayName: config.displayName,
    description: config.description,
    isActive: true,
    createdAt: new Date(),
    updatedAt: new Date()
  };
}
```

**Tab Opening**:
```typescript
// User clicks "VRAM Calculator" in sidebar or menu
openContent('/tools/vram-calculator');

// MainWidget creates new tab
const contentInfo = await contentManager.getContentByPath('/tools/vram-calculator');
const tab = createTab(contentInfo);
const widget = document.createElement('vram-calculator-widget');
tab.appendChild(widget);
```

### Phase 4: Training Monitor Integration

**Future enhancement** - When user starts training job:
```typescript
// Open training monitor tab automatically
const sessionId = trainingResult.sessionId;
openContent(`/training/${sessionId}`);

// Monitor shows:
// - Real-time logs
// - Progress bar
// - Actual VRAM usage vs estimate
// - Performance metrics (tok/sec)
// - Cost tracker
```

---

## UI Mockup (Text-based)

```
┌─ continuum ──────────────────────────────────────────────────────────┐
│ 📂 JTAG v1.0                                                         │
├──────────────────────────────────────────────────────────────────────┤
│ [General] [VRAM Calculator] [Training Monitor]  ← Tabs like VSCode  │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  VRAM Calculator for LoRA Fine-Tuning                                │
│                                                                       │
│  ┌─ Model Selection ─────────────────────────────────────────┐      │
│  │ Model: [DeepSeek-R1 1.5B ▼]                                │      │
│  │ Parameters: 1.5B  |  Architecture: Transformer             │      │
│  │ Context Length: 64K                                        │      │
│  └────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌─ Training Configuration ──────────────────────────────────┐      │
│  │ LoRA Rank (r):     [16     ] (1-128)                       │      │
│  │ Batch Size:        [4      ] (1-128)                       │      │
│  │ Sequence Length:   [2048   ] (512-131072)                  │      │
│  │ Precision:         [4-bit ▼] (4-bit, 8-bit, 16-bit, 32-bit)│      │
│  └────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌─ Hardware Selection ──────────────────────────────────────┐      │
│  │ Platform: [Apple Silicon ▼]                                │      │
│  │ Device:   [M2 Pro (16GB) ▼]                                │      │
│  └────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌─ Optimizations ───────────────────────────────────────────┐      │
│  │ [✓] Flash Attention (45% VRAM savings)                     │      │
│  │ [✓] Gradient Checkpointing (70% VRAM savings)              │      │
│  │ [✓] 8-bit Optimizer (75% VRAM savings)                     │      │
│  │ [ ] CPU Offloading (dynamic VRAM savings)                  │      │
│  └────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌─ Memory Estimate ─────────────────────────────────────────┐      │
│  │ Total VRAM: 5.75 GB / 16 GB (35.9%)                        │      │
│  │                                                             │      │
│  │ [████████████████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░]    │      │
│  │                                                             │      │
│  │ Breakdown:                                                  │      │
│  │   Base Model:   3.00 GB (52.1%)                            │      │
│  │   Activations:  1.41 GB (24.5%)                            │      │
│  │   Framework:    1.31 GB (22.7%)                            │      │
│  │   LoRA Weights: 0.04 GB (0.7%)                             │      │
│  │                                                             │      │
│  │ ✅ Training will fit in available VRAM                     │      │
│  └────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  ┌─ Performance Estimate ────────────────────────────────────┐      │
│  │ Training Speed:  ~18 tokens/sec                            │      │
│  │ Dataset Size:    10,000 examples (avg 512 tokens)          │      │
│  │ Estimated Time:  ~1.5 hours                                │      │
│  │ Cost (DeepSeek): $0.006 ($0.004/hour × 1.5h)              │      │
│  └────────────────────────────────────────────────────────────┘      │
│                                                                       │
│  [Start Training]  [Export Config]  [Save Preset]                   │
│                                                                       │
└───────────────────────────────────────────────────────────────────────┘
```

---

## Data Sources

### Model Database
**Source**: `system/genome/fine-tuning/docs/POPULAR-MODELS-BY-PROVIDER.md`
```typescript
const modelDatabase = {
  'deepseek-r1-1.5b': {
    name: 'DeepSeek-R1 1.5B',
    provider: 'deepseek',
    parameters: 1.5e9,
    contextLength: 64 * 1024,
    architecture: 'transformer',
    supportedPrecisions: ['4-bit', '8-bit', '16-bit'],
    // ... more metadata
  },
  // ... 100+ more models
};
```

### Hardware Database
```typescript
const hardwareDatabase = {
  appleSilicon: {
    'm2-pro-16gb': { vram: 16 * 1024, bandwidth: 200, tokensPerSec: 18 },
    'm3-max-96gb': { vram: 96 * 1024, bandwidth: 400, tokensPerSec: 45 },
    // ...
  },
  nvidia: {
    'rtx-4090': { vram: 24 * 1024, bandwidth: 1008, tokensPerSec: 120 },
    'h100-80gb': { vram: 80 * 1024, bandwidth: 3350, tokensPerSec: 500 },
    // ...
  }
};
```

### Provider Costs
**Source**: `daemons/ai-provider-daemon/adapters/*/shared/*BaseConfig.ts`
```typescript
// From OpenAIBaseConfig.ts
costPer1kTokens: { input: 0.003, output: 0.006 }

// From DeepSeekBaseConfig.ts
costPer1kTokens: { input: 0.00027, output: 0.00108 }

// Calculate training cost
const tokensProcessed = datasetSize * avgTokensPerExample * epochs;
const costPerToken = provider.costPer1kTokens.input / 1000;
const totalCost = tokensProcessed * costPerToken;
```

---

## Benefits

### For Users
1. **Plan before spending** - Know exact VRAM requirements before starting training
2. **Hardware recommendations** - Find cheapest hardware that fits their needs
3. **Cost estimation** - Budget for cloud training costs
4. **Optimization guidance** - Learn which toggles to enable

### For Platform
1. **Differentiation** - No other LoRA marketplace has integrated VRAM calculator
2. **Education** - Demystifies LoRA training for newcomers
3. **Trust** - Shows we understand the technical details
4. **Upsell** - When user sees "won't fit", suggest cloud providers we support

### For LoRA Marketplace
1. **Seller enablement** - Helps sellers plan their training infrastructure
2. **Buyer transparency** - Buyers can see training costs in listings
3. **Quality signal** - High VRAM = more compute = potentially better adapters
4. **Discovery** - "Models trainable on your hardware" filter

---

## Future Enhancements

### Phase 5: Training Presets
```typescript
const presets = {
  'apple-m2-budget': {
    precision: '4-bit',
    batchSize: 2,
    loraRank: 8,
    flashAttention: true,
    gradientCheckpointing: true
  },
  'nvidia-4090-fast': {
    precision: '16-bit',
    batchSize: 16,
    loraRank: 32,
    flashAttention: true,
    gradientCheckpointing: false
  }
};
```

### Phase 6: Real-time Monitoring
- **During training**: Show actual VRAM usage vs estimate
- **Accuracy tracking**: Improve calculator formulas based on real data
- **Warnings**: Alert if VRAM usage exceeds estimate

### Phase 7: Multi-GPU Support
- **Calculate sharding**: How to split model across multiple GPUs
- **Communication overhead**: Estimate inter-GPU bandwidth requirements
- **Cost optimization**: When is multi-GPU cheaper than single large GPU?

---

## Testing Strategy

### Unit Tests
```bash
npx vitest tests/unit/VramCalculator.test.ts
```
Test cases:
- Memory calculation accuracy (compare to apxml.com results)
- Optimization multipliers correct
- Feasibility checks work
- Recommendations are sensible

### Integration Tests
```bash
npx vitest tests/integration/vram-calculator-widget.test.ts
```
Test cases:
- Widget loads in tab
- Model dropdown populates from providers
- Hardware selection works
- Real-time estimate updates on input change

### Visual Regression Tests
```bash
./jtag interface/screenshot --querySelector="vram-calculator-widget" --filename="calculator-baseline.png"
```

---

## Status: Planning Phase

**Next steps**:
1. Get user confirmation on integration approach
2. Create widget file structure
3. Implement VramCalculatorLogic.ts with calculation formulas
4. Build VramCalculatorWidget.ts UI
5. Update ContentTypes.ts for 'tool' content type
6. Wire up content routing
7. Test with real provider data

**Decision needed**: Should we build this now or wait until after more providers are added?
