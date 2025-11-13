# LoRA Adapter Integration Strategy

**Philosophy**: Keep test infrastructure as permanent test bench for isolated adapter development

## The Pattern: Test-Driven Adapter Development

### Current State (Good Foundation)
```
api-tests/
├── BaseRemoteAPITest.ts          # Isolated test base class
├── test-openai.ts                # Isolated OpenAI test
├── test-deepseek.ts              # Isolated DeepSeek test
├── test-fireworks.ts             # Isolated Fireworks test
├── test-together.ts              # Isolated Together test
├── test-all.sh                   # Test runner
└── /tmp/test-training-*.jsonl    # Test data
```

### Target State (Dual-Mode Adapters)
```
adapters/
├── OpenAILoRAAdapter.ts          # JTAG integration (uses shared core)
├── DeepSeekLoRAAdapter.ts        # JTAG integration (extends OpenAI)
└── shared/
    ├── RemoteAPICore.ts          # SHARED core logic (extracted from tests)
    └── RemoteAPITypes.ts         # SHARED types

api-tests/
├── BaseRemoteAPITest.ts          # Uses RemoteAPICore internally
├── test-openai.ts                # Isolated test (uses RemoteAPICore)
├── test-deepseek.ts              # Isolated test (uses RemoteAPICore)
├── test-all.sh                   # Still works independently!
└── INTEGRATION-STRATEGY.md       # This file
```

**Key insight**: Extract shared logic into `RemoteAPICore.ts` that BOTH tests AND adapters use.

---

## Implementation Plan

### Phase 1: Extract Shared Core (~1 hour)

Create `adapters/shared/RemoteAPICore.ts` with the universal pattern:

```typescript
/**
 * RemoteAPICore - Shared logic for all remote fine-tuning APIs
 *
 * Used by:
 * 1. JTAG adapters (OpenAILoRAAdapter, DeepSeekLoRAAdapter, etc.)
 * 2. Isolated test scripts (test-openai.ts, test-deepseek.ts, etc.)
 *
 * Philosophy: Write once, test isolated, integrate everywhere
 */

export abstract class RemoteAPICore {
  // Abstract methods (provider-specific)
  protected abstract uploadTrainingData(jsonlPath: string): Promise<UploadResult>;
  protected abstract createFineTuningJob(uploadResult: UploadResult): Promise<string>;
  protected abstract checkJobStatus(jobId: string): Promise<JobStatus>;
  protected abstract isComplete(status: JobStatus): boolean;
  protected abstract isFailed(status: JobStatus): boolean;

  // Shared implementation (universal pattern)
  protected async waitForCompletion(jobId: string): Promise<string> {
    // Poll every 5s until complete (same for all providers)
  }

  protected async saveAdapterMetadata(...): Promise<string> {
    // Save adapter JSON (same for all providers)
  }

  protected readTrainingFile(jsonlPath: string): {content: string, lines: string[]} {
    // Read and validate JSONL (same for all providers)
  }

  protected async fetch(endpoint: string, options: RequestInit): Promise<Response> {
    // Authenticated fetch (same for all providers)
  }

  protected async handleResponse<T>(response: Response): Promise<T> {
    // Error handling (same for all providers)
  }
}
```

**Extract from**: `BaseRemoteAPITest.ts` (lines 98-319)

### Phase 2: Update Test Infrastructure (~30 min)

**BaseRemoteAPITest.ts** - Use RemoteAPICore internally:

```typescript
import { RemoteAPICore } from '../../shared/RemoteAPICore';

export abstract class BaseRemoteAPITest extends RemoteAPICore {
  // Add test-specific orchestration
  async runTest(): Promise<TestResult> {
    console.log('🚀 Testing...');

    // Use inherited RemoteAPICore methods
    const uploadResult = await this.uploadTrainingData(this.config.trainingFile);
    const jobId = await this.createFineTuningJob(uploadResult);
    const modelId = await this.waitForCompletion(jobId);
    const metadataPath = await this.saveAdapterMetadata(...);

    return { success: true, modelId, metadataPath };
  }
}
```

**test-openai.ts** - No changes needed! Still extends BaseRemoteAPITest.

**Result**: Tests still work independently, but now use shared core.

### Phase 3: Create JTAG Adapters (~2 hours)

**OpenAILoRAAdapter.ts** - JTAG integration:

```typescript
import { BaseLoRATrainer } from '../../shared/BaseLoRATrainer';
import { RemoteAPICore } from './shared/RemoteAPICore';
import type { LoRATrainingRequest, LoRATrainingResult } from '../../shared/FineTuningTypes';

export class OpenAILoRAAdapter extends BaseLoRATrainer {
  readonly providerId = 'openai';
  private core: OpenAICore; // Uses RemoteAPICore

  supportsFineTuning(): boolean {
    return !!process.env.OPENAI_API_KEY;
  }

  async trainLoRA(request: LoRATrainingRequest): Promise<LoRATrainingResult> {
    this.validateRequest(request);

    // 1. Export dataset to JSONL
    const jsonlPath = await this.exportDatasetToJSONL(request.dataset);

    // 2-4. Use RemoteAPICore (same logic as tests!)
    const uploadResult = await this.core.uploadTrainingData(jsonlPath);
    const jobId = await this.core.createFineTuningJob(uploadResult);
    const modelId = await this.core.waitForCompletion(jobId);
    await this.core.saveAdapterMetadata(modelId, request);

    return {
      success: true,
      modelId,
      metrics: { ... },
      timestamp: Date.now()
    };
  }

  // Helper: Convert JTAG dataset to JSONL
  private async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    const tempPath = path.join(os.tmpdir(), `openai-training-${Date.now()}.jsonl`);
    const jsonl = dataset.examples.map(ex => JSON.stringify({
      messages: [
        { role: 'system', content: dataset.systemPrompt },
        { role: 'user', content: ex.input },
        { role: 'assistant', content: ex.output }
      ]
    })).join('\n');
    await fs.promises.writeFile(tempPath, jsonl, 'utf-8');
    return tempPath;
  }
}

// OpenAICore - Specific implementation of RemoteAPICore
class OpenAICore extends RemoteAPICore {
  // Implements abstract methods (copy from test-openai.ts)
  protected async uploadTrainingData(jsonlPath: string): Promise<UploadResult> {
    // Same as test-openai.ts lines 38-72
  }

  protected async createFineTuningJob(uploadResult: UploadResult): Promise<string> {
    // Same as test-openai.ts lines 78-112
  }

  protected async checkJobStatus(jobId: string): Promise<JobStatus> {
    // Same as test-openai.ts lines 118-133
  }

  protected isComplete(status: JobStatus): boolean {
    return status.state === 'succeeded';
  }

  protected isFailed(status: JobStatus): boolean {
    return status.state === 'failed' || status.state === 'cancelled';
  }
}
```

**DeepSeekLoRAAdapter.ts** - Extends OpenAI:

```typescript
import { OpenAILoRAAdapter } from './OpenAILoRAAdapter';

export class DeepSeekLoRAAdapter extends OpenAILoRAAdapter {
  readonly providerId = 'deepseek';

  // Override API config
  protected getApiBase(): string {
    return 'https://api.deepseek.com/v1';
  }

  protected getApiKey(): string | undefined {
    return process.env.DEEPSEEK_API_KEY;
  }

  supportsFineTuning(): boolean {
    return !!this.getApiKey();
  }
}
```

**Result**: JTAG adapters work, share code with tests.

---

## Key Benefits of This Approach

### 1. Isolated Testing (Short-Circuit)
```bash
# Test adapter logic WITHOUT JTAG overhead
cd api-tests
./test-openai.ts   # Direct test, ~10 seconds
./test-all.sh      # All providers, ~2 minutes
```

**No need to**:
- Start JTAG system
- Wait for npm start (90s)
- Navigate UI
- Check logs

**Just**: Run test, see result, iterate fast.

### 2. Dual-Mode Development

**Mode 1: Isolated Development**
```bash
# Working on OpenAI adapter
cd api-tests
vim ../OpenAILoRAAdapter.ts  # Edit adapter
npx tsx test-openai.ts       # Test immediately
```

**Mode 2: Integration Testing**
```bash
# Test full JTAG integration
npm start                    # Deploy
./jtag genome/train          # Test end-to-end
```

### 3. Code Reuse Across Modes

```
RemoteAPICore (shared)
├── Used by: BaseRemoteAPITest (tests)
├── Used by: OpenAILoRAAdapter (JTAG)
└── Used by: DeepSeekLoRAAdapter (JTAG)

One implementation, three use cases!
```

### 4. Regression Prevention

When you change adapter logic:
1. Run isolated test first (`./test-openai.ts`)
2. If test passes, integration will likely work
3. If test fails, fix before deploying

**Saves time**: Catch issues in 10s, not 90s.

### 5. Documentation Through Tests

New developer wants to add Groq adapter:
1. Read `test-openai.ts` - see exact API pattern
2. Copy to `test-groq.ts` - implement provider-specific methods
3. Run `./test-groq.ts` - verify it works
4. Copy to `GroqLoRAAdapter.ts` - integrate into JTAG
5. Run `./jtag genome/train --provider=groq` - done!

**Tests ARE documentation** - show exactly how API works.

---

## File Structure After Integration

```
system/genome/fine-tuning/server/adapters/
├── shared/
│   ├── RemoteAPICore.ts           # NEW: Shared logic
│   └── RemoteAPITypes.ts          # NEW: Shared types
│
├── OpenAILoRAAdapter.ts           # UPDATED: Uses RemoteAPICore
├── DeepSeekLoRAAdapter.ts         # UPDATED: Extends OpenAI
├── FireworksLoRAAdapter.ts        # FUTURE: Uses RemoteAPICore
├── TogetherLoRAAdapter.ts         # FUTURE: Uses RemoteAPICore
├── AWSBedrockLoRAAdapter.ts       # FUTURE: Different pattern
│
└── api-tests/                     # KEPT: Permanent test bench
    ├── BaseRemoteAPITest.ts       # UPDATED: Uses RemoteAPICore
    ├── test-openai.ts             # UNCHANGED: Still works!
    ├── test-deepseek.ts           # UNCHANGED: Still works!
    ├── test-fireworks.ts          # UNCHANGED: Still works!
    ├── test-together.ts           # UNCHANGED: Still works!
    ├── test-all.sh                # UNCHANGED: Still works!
    ├── STATUS.md                  # Documentation
    └── INTEGRATION-STRATEGY.md    # This file
```

---

## Implementation Checklist

### Step 1: Extract Shared Core
- [ ] Create `adapters/shared/RemoteAPICore.ts`
- [ ] Create `adapters/shared/RemoteAPITypes.ts`
- [ ] Extract universal pattern from `BaseRemoteAPITest.ts`
- [ ] Add JTAG-specific helpers (dataset conversion)

### Step 2: Update Test Infrastructure
- [ ] Update `BaseRemoteAPITest.ts` to extend `RemoteAPICore`
- [ ] Run `./test-all.sh` - verify tests still work
- [ ] Update `STATUS.md` with new architecture

### Step 3: Implement OpenAI Adapter
- [ ] Update `OpenAILoRAAdapter.ts` to use `RemoteAPICore`
- [ ] Implement `OpenAICore` class (copy from test)
- [ ] Add dataset-to-JSONL conversion
- [ ] Test: `npx tsx api-tests/test-openai.ts` (isolated)
- [ ] Test: `npm start && ./jtag genome/train --provider=openai` (integrated)

### Step 4: Implement DeepSeek Adapter
- [ ] Update `DeepSeekLoRAAdapter.ts` to extend `OpenAILoRAAdapter`
- [ ] Override API config methods
- [ ] Test: `npx tsx api-tests/test-deepseek.ts` (isolated)
- [ ] Test: `./jtag genome/train --provider=deepseek` (integrated)

### Step 5: Documentation
- [ ] Update `IMMEDIATE-ROADMAP.md` with new architecture
- [ ] Document dual-mode testing workflow
- [ ] Add examples to `STATUS.md`

---

## Testing Workflow

### Daily Development
```bash
# 1. Make changes to adapter
vim adapters/OpenAILoRAAdapter.ts

# 2. Test isolated (fast!)
npx tsx api-tests/test-openai.ts

# 3. If pass, test integration
npm start
./jtag genome/train --provider=openai --dryRun=false

# 4. Commit when both pass
git add .
git commit -m "feat: OpenAI adapter working"
```

### Adding New Provider
```bash
# 1. Create isolated test first
cp api-tests/test-openai.ts api-tests/test-groq.ts
vim api-tests/test-groq.ts  # Implement Groq-specific methods

# 2. Test isolated until working
npx tsx api-tests/test-groq.ts

# 3. Copy to JTAG adapter
cp adapters/OpenAILoRAAdapter.ts adapters/GroqLoRAAdapter.ts
vim adapters/GroqLoRAAdapter.ts  # Adapt for Groq

# 4. Test integration
npm start
./jtag genome/train --provider=groq

# 5. Add to test suite
vim api-tests/test-all.sh  # Add groq test
```

### Debugging Issues
```bash
# Always start with isolated test
npx tsx api-tests/test-openai.ts 2>&1 | tee debug.log

# If isolated test fails, fix core logic first
# If isolated test passes but integration fails, check JTAG integration layer
```

---

## Success Criteria

**Phase 1 Complete When**:
- ✅ `RemoteAPICore.ts` extracted with universal pattern
- ✅ Tests still work: `./test-all.sh` passes
- ✅ Code shared between tests and adapters

**Phase 2 Complete When**:
- ✅ `OpenAILoRAAdapter.ts` uses `RemoteAPICore`
- ✅ Isolated test passes: `./test-openai.ts` succeeds
- ✅ Integration test passes: `./jtag genome/train --provider=openai` succeeds

**Phase 3 Complete When**:
- ✅ `DeepSeekLoRAAdapter.ts` extends `OpenAILoRAAdapter`
- ✅ Isolated test passes: `./test-deepseek.ts` succeeds
- ✅ Integration test passes: `./jtag genome/train --provider=deepseek` succeeds

**Final Validation**:
- ✅ Can add new provider by copying test, then adapter
- ✅ Can test adapter changes in <10s (isolated mode)
- ✅ Can test full integration in ~120s (JTAG mode)
- ✅ Code reuse maintained (87.5% average)

---

## The Pattern Applied Elsewhere

This same pattern works for other complex adapters:

**Current**: Ollama local training (future Phase 6)
```
adapters/
├── OllamaLoRAAdapter.ts
└── api-tests/
    └── test-ollama-local.sh  # Test llama.cpp directly
```

**Current**: AWS Bedrock (future Phase 7)
```
adapters/
├── AWSBedrockLoRAAdapter.ts
└── api-tests/
    └── test-aws-bedrock.ts  # Test S3 upload + Bedrock API
```

**Key principle**: If it's complex, make it testable in isolation first.

---

## Bottom Line

**What you said**: "keep your tests working... independently callable so they can be isolated and worked on"

**What we'll do**:
1. Extract `RemoteAPICore.ts` with shared logic
2. Tests use core → still work independently
3. Adapters use core → share same logic
4. New workflow: Test isolated (10s) → Test integrated (120s) → Ship

**Result**: Fast iteration, high confidence, easy debugging.

**Next step**: Extract `RemoteAPICore.ts` and update `BaseRemoteAPITest.ts` to use it.
