# Fine-Tuning API Tests

Isolated tests for all remote fine-tuning providers using shared base class for maximum code reuse.

## Architecture

### The Universal Pattern

All fine-tuning workflows follow a **4-step pattern**:

```
1. UPLOAD  → Get training data to provider
2. CREATE  → Start fine-tuning job
3. POLL    → Wait for completion
4. SAVE    → Store adapter metadata
```

**Key insight**: The WHAT is always the same, only the HOW differs.

### Code Reuse Strategy

```
BaseRemoteAPITest (abstract)
├── Shared logic: polling, metadata, orchestration
├── Abstract methods: upload, create, check status
└── Utility methods: fetch, sleep, error handling

OpenAIFineTuningTest extends BaseRemoteAPITest
├── FormData file upload
├── OpenAI API format
└── Reference implementation

DeepSeekFineTuningTest extends OpenAIFineTuningTest
├── 95% code reuse (just config changes)
└── OpenAI-compatible API

TogetherFineTuningTest extends OpenAIFineTuningTest
├── 95% code reuse (just config changes)
└── OpenAI-compatible API

FireworksFineTuningTest extends BaseRemoteAPITest
├── 60% code reuse (different upload)
└── Inline data submission
```

## Files

- **BaseRemoteAPITest.ts** - Shared base class with common logic
- **test-openai.ts** - OpenAI implementation (reference)
- **test-deepseek.ts** - DeepSeek (extends OpenAI)
- **test-together.ts** - Together AI (extends OpenAI)
- **test-fireworks.ts** - Fireworks AI (extends Base, different upload)
- **test-all.sh** - Master test runner script

## Usage

### Test Individual Provider

```bash
# OpenAI
npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-openai.ts

# DeepSeek (27x cheaper!)
npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-deepseek.ts

# Fireworks (inline data)
npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-fireworks.ts

# Together (OpenAI-compatible)
npx tsx system/genome/fine-tuning/server/adapters/api-tests/test-together.ts
```

### Test All Providers

```bash
# Run master test script
./system/genome/fine-tuning/server/adapters/api-tests/test-all.sh
```

## Requirements

### API Keys (Environment Variables)

```bash
export OPENAI_API_KEY="sk-..."
export DEEPSEEK_API_KEY="sk-..."
export FIREWORKS_API_KEY="..."
export TOGETHER_API_KEY="..."
```

These are loaded from `~/.continuum/config.json` by the secret manager.

### Training Data

```bash
# Required: minimal training dataset
/tmp/test-training-minimal.jsonl
```

Example format (5 examples):
```jsonl
{"messages":[{"role":"system","content":"You are a helpful assistant."},{"role":"user","content":"What is Python?"},{"role":"assistant","content":"Python is a high-level programming language."}]}
{"messages":[{"role":"system","content":"You are a helpful assistant."},{"role":"user","content":"What is TypeScript?"},{"role":"assistant","content":"TypeScript is a typed superset of JavaScript."}]}
...
```

## Provider Differences

| Provider | Upload Method | Status Value | Model ID Field | Cost/1K Examples |
|----------|---------------|--------------|----------------|------------------|
| OpenAI | FormData POST | `succeeded` | `fine_tuned_model` | $8.10 |
| DeepSeek | FormData POST | `succeeded` | `fine_tuned_model` | $0.30 (27x cheaper!) |
| Together | FormData POST | `succeeded` | `fine_tuned_model` | ~$5.00 |
| Fireworks | Inline JSON | `completed` | `output_model` or `model_id` | ~$4.00 |

## Code Reuse Metrics

- **OpenAI**: 100% (reference implementation, ~200 lines)
- **DeepSeek**: 95% reuse (~10 lines, just config)
- **Together**: 95% reuse (~10 lines, just config)
- **Fireworks**: 60% reuse (~80 lines, different upload)

**Average: 87.5% code reuse**

## What This Tests

✅ **API Connectivity** - Can we reach the provider?
✅ **Authentication** - Do API keys work?
✅ **Upload Strategy** - FormData vs inline data
✅ **Job Creation** - Can we start training?
✅ **Status Polling** - Can we track progress?
✅ **Completion Detection** - Do we recognize success/failure?
✅ **Metadata Storage** - Do we save results?

## What This Does NOT Test

❌ **JTAG Integration** - These are isolated from JTAG infrastructure
❌ **Adapter Classes** - Test providers directly, not adapter wrappers
❌ **Genome Commands** - No `./jtag genome/train` testing
❌ **PersonaUser** - No persona-level testing

**Philosophy**: "Start isolated first and work your way back into our API"

## Next Steps

Once these tests pass:

1. **Integrate into Adapters** - Copy working code into `*LoRAAdapter.ts` files
2. **Create BaseRemoteAPIAdapter** - Extract shared logic into base class
3. **Update Adapter Implementations** - Make adapters extend base class
4. **Test with JTAG** - Run `./jtag genome/train` end-to-end
5. **Phase 2: AWS Bedrock** - Add support for Anthropic Claude fine-tuning

## Example Output

```bash
$ ./system/genome/fine-tuning/server/adapters/api-tests/test-all.sh

🚀 Testing ALL Fine-Tuning Providers (API Tests)
================================================

Architecture:
  - BaseRemoteAPITest: Shared logic (polling, metadata, errors)
  - Provider tests: Only implement upload/create/status methods
  - Code reuse: 60-95% depending on provider compatibility

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Test 1/4: OpenAI Fine-Tuning
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

🚀 openai Fine-Tuning Test (API)
==================================================

✅ API key found: sk-proj-YK...
📤 Step 1: Uploading training file to OpenAI...
   File: /tmp/test-training-minimal.jsonl
   Examples: 5
✅ File uploaded successfully
   File ID: file-abc123
   Bytes: 1234

🔧 Step 2: Creating fine-tuning job...
   File ID: file-abc123
   Base Model: gpt-4o-mini-2024-07-18
   Epochs: 1
✅ Job created successfully
   Job ID: ftjob-xyz789
   Status: validating_files

⏳ Step 3: Waiting for job completion...
   Job ID: ftjob-xyz789
   Max wait: 30.0 minutes

   [0s] Status: validating_files
   [15s] Status: running
   [180s] Status: succeeded

✅ Job completed successfully!
   Fine-tuned Model: ft:gpt-4o-mini:...:abc123
   Training Time: 180s

💾 Step 4: Saving adapter metadata...
✅ Metadata saved
   Path: /tmp/openai-adapter-1699999999.json

🎉 SUCCESS! Fine-tuning completed
==================================================
Model ID: ft:gpt-4o-mini:...:abc123
Metadata: /tmp/openai-adapter-1699999999.json
Training Time: 180s

✅ OpenAI test PASSED

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
TEST SUMMARY
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Passed:  4
Failed:  0
Skipped: 0

Total tests: 4

Code Reuse Metrics:
  - OpenAI: 100% (reference implementation)
  - DeepSeek: 95% (extends OpenAI, just config changes)
  - Together: 95% (extends OpenAI, just config changes)
  - Fireworks: 60% (shares BaseRemoteAPITest, different upload)

Average code reuse: 87.5%

✅ All executed tests passed!
```

## Design Principles

1. **Maximum Code Reuse** - Extract common patterns into base class
2. **Isolated Testing** - No JTAG dependencies, test APIs directly
3. **Clear Abstraction** - Provider-specific logic clearly separated
4. **Fail Fast** - Validate API keys and training data upfront
5. **Informative Output** - Show what's happening at each step
6. **Easy Integration** - Working code can be copied into adapters

## Related Documentation

- `/tmp/adapter-pattern-analysis.md` - Original pattern discovery
- `/tmp/lora-adapter-abstraction-strategy.md` - Complete abstraction design
- `../BaseLoRATrainer.ts` - Base adapter interface (to be implemented)
- `../OpenAILoRAAdapter.ts` - Current MVP stub (to be updated)
