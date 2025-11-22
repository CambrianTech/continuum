# Adapter Consolidation Design

**Goal**: Merge inference and fine-tuning adapters into unified provider adapters that handle BOTH capabilities.

**Problem**: Current architecture has separate adapters:
- `daemons/ai-provider-daemon/adapters/together/` - Inference only
- `system/genome/fine-tuning/server/adapters/TogetherLoRAAdapter.ts` - Fine-tuning only

**Why this is bad**:
- Code duplication (both parse same models, use same API key, handle same auth)
- User confusion (which adapter to use?)
- Maintenance burden (update pricing in two places)
- Missed optimization opportunities (could cache models list between capabilities)

---

## Architecture Analysis

### Current Inheritance Hierarchies

**Inference side**:
```
BaseAIProviderAdapter (abstract)
  └── BaseOpenAICompatibleAdapter (abstract)
      └── TogetherAIAdapter (concrete)
```

**Fine-tuning side**:
```
BaseLoRATrainer (abstract)
  └── BaseLoRATrainerServer (abstract)
      └── TogetherLoRAAdapter (concrete)
```

**Key insight**: These are SEPARATE hierarchies. Cannot use simple inheritance to merge them.

### Composition Strategy

Instead of inheritance, use **composition** + **interface implementation**:

```typescript
class UnifiedTogetherAdapter
  extends BaseOpenAICompatibleAdapter // For inference
  implements ILoRAFineTuner {           // For fine-tuning

  private fineTuningImpl: TogetherFineTuningImpl;

  // Inference methods (inherited from base)
  async generateText(...) { ... }
  async getAvailableModels() { ... }

  // Fine-tuning methods (delegated to impl)
  async trainLoRA(request) {
    return this.fineTuningImpl.trainLoRA(request);
  }

  async checkStatus(sessionId) {
    return this.fineTuningImpl.checkStatus(sessionId);
  }

  supportsFineTuning() { return true; }
}
```

---

## Migration Strategy

### Phase 1: Create Interface + Implementation Classes

1. **Define ILoRAFineTuner interface** (system/genome/fine-tuning/shared/)
   ```typescript
   export interface ILoRAFineTuner {
     trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult>;
     checkStatus(sessionId: UUID): Promise<TrainingStatus>;
     supportsFineTuning(): boolean;
     getFineTuningCapabilities(): FineTuningCapabilities;
     estimateTrainingCost(exampleCount: number): number;
     estimateTrainingTime(exampleCount: number, epochs: number): number;
   }
   ```

2. **Create TogetherFineTuningImpl** (internal implementation class)
   - Contains all fine-tuning logic from current TogetherLoRAAdapter
   - Implements async handle pattern (_startTraining, _queryStatus)
   - Reuses API client from parent adapter (no duplicate auth logic)

### Phase 2: Extend Inference Adapter with Fine-Tuning

Update `TogetherAIAdapter`:
```typescript
export class TogetherAIAdapter
  extends BaseOpenAICompatibleAdapter
  implements ILoRAFineTuner {

  private fineTuning: TogetherFineTuningImpl;

  constructor(apiKey?: string) {
    super({ /* inference config */ });

    // Initialize fine-tuning implementation
    this.fineTuning = new TogetherFineTuningImpl(
      this.providerId,
      this.config.apiKey,
      this.config.baseUrl
    );
  }

  // Fine-tuning methods (delegated)
  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    return this.fineTuning.trainLoRA(request);
  }

  async checkStatus(sessionId: UUID): Promise<TrainingStatus> {
    return this.fineTuning.checkStatus(sessionId);
  }

  supportsFineTuning(): boolean {
    return this.fineTuning.supportsFineTuning();
  }

  getFineTuningCapabilities(): FineTuningCapabilities {
    return this.fineTuning.getFineTuningCapabilities();
  }

  estimateTrainingCost(exampleCount: number): number {
    return this.fineTuning.estimateTrainingCost(exampleCount);
  }

  estimateTrainingTime(exampleCount: number, epochs: number): number {
    return this.fineTuning.estimateTrainingTime(exampleCount, epochs);
  }
}
```

### Phase 3: Deprecate Old Fine-Tuning Adapters

1. Mark `system/genome/fine-tuning/server/adapters/TogetherLoRAAdapter.ts` as deprecated
2. Update GenomeTrainServerCommand to use new unified adapters
3. Remove old adapter after migration verified

---

## Benefits of This Design

### Code Reduction
**Before**:
- TogetherAIAdapter.ts: 69 lines
- TogetherLoRAAdapter.ts: 526 lines
- **Total: 595 lines**

**After**:
- TogetherAIAdapter.ts: ~150 lines (69 + delegation methods)
- TogetherFineTuningImpl.ts: ~400 lines (shared helper methods)
- **Total: ~550 lines (45 lines saved)**

More importantly: **ZERO duplication** of auth, config, model parsing, error handling.

### Shared Capabilities
- Both inference and fine-tuning use same API key
- Both share model list (no duplicate fetching)
- Both use same pricing data
- Both benefit from health monitoring
- Both handled by AIProviderDaemon (single registration point)

### User Experience
```typescript
// Before (confusing - two adapters for one provider!)
const inference = new TogetherAIAdapter();
const fineTuning = new TogetherLoRAAdapter();

await inference.generateText({ ... });
await fineTuning.trainLoRA({ ... });

// After (natural - one adapter, two capabilities)
const together = new TogetherAIAdapter();

await together.generateText({ ... });
await together.trainLoRA({ ... });
```

---

## Implementation Plan

### Step 1: Create Interface (5 minutes)
File: `system/genome/fine-tuning/shared/ILoRAFineTuner.ts`

### Step 2: Extract Implementation Class (15 minutes)
File: `system/genome/fine-tuning/server/impl/TogetherFineTuningImpl.ts`
- Copy logic from TogetherLoRAAdapter
- Remove BaseLoRATrainerServer inheritance
- Accept providerId, apiKey, baseUrl in constructor
- Keep async handle pattern (_startTraining, _queryStatus)

### Step 3: Extend Inference Adapter (10 minutes)
File: `daemons/ai-provider-daemon/adapters/together/shared/TogetherAIAdapter.ts`
- Add `implements ILoRAFineTuner`
- Initialize TogetherFineTuningImpl in constructor
- Add delegation methods (6 one-liners)

### Step 4: Update Command Registration (5 minutes)
File: `commands/genome/train/server/GenomeTrainServerCommand.ts`
- Import from AIProviderDaemon instead of fine-tuning/adapters
- Cast adapter to `ILoRAFineTuner` type

### Step 5: Test & Deprecate (10 minutes)
1. Run `npm start` (verify compilation)
2. Test inference: `./jtag ai/send "test"`
3. Test fine-tuning: `./jtag genome/train --provider=together`
4. Mark old adapter deprecated
5. Update PROVIDER-STATUS.md

**Total time estimate: 45 minutes**

---

## Rollout to Other Providers

Once Together consolidation proven:

### OpenAI (already mostly consolidated)
- OpenAIAdapter already exists for inference
- OpenAILoRAAdapter just needs to become OpenAIFineTuningImpl
- 10 minutes

### Fireworks
- Same pattern as Together
- 15 minutes (slightly different API)

### Mistral
- Same pattern as Together
- 15 minutes

### Anthropic (future)
- Waiting for official fine-tuning API
- Will follow same pattern when available

**Total consolidation time: ~2 hours for all providers**

---

## Why Composition > Inheritance

### Problem with Multiple Inheritance
TypeScript doesn't support multiple inheritance. Can't do:
```typescript
class UnifiedAdapter
  extends BaseAIProviderAdapter
  extends BaseLoRATrainer { } // ❌ Syntax error
```

### Problem with Complex Hierarchy
Could create intermediate class:
```typescript
abstract class BaseUnifiedAdapter
  extends BaseAIProviderAdapter {
  // Mix in fine-tuning methods
}
```
But this forces ALL providers to include fine-tuning (even if not supported).

### Solution: Composition
```typescript
class ProviderAdapter extends BaseAIProviderAdapter {
  private fineTuning?: IFineTuningImpl; // Optional capability
}
```

**Benefits**:
- Clean separation of concerns
- Optional capability (not all providers support fine-tuning)
- Easy to test (mock implementation)
- Follows Interface Segregation Principle

---

## Success Criteria

✅ Compilation succeeds (0 TypeScript errors)
✅ Inference still works (existing tests pass)
✅ Fine-tuning still works (create job + check status)
✅ Code duplication eliminated (auth, config, models shared)
✅ Documentation updated (PROVIDER-STATUS.md, this file)
✅ Old adapters marked deprecated

---

## Status

- [ ] Phase 1: Create interface + implementation
- [ ] Phase 2: Extend inference adapter
- [ ] Phase 3: Test & deprecate old code
- [ ] Phase 4: Rollout to other providers

**Next**: Begin Phase 1 - create ILoRAFineTuner interface
