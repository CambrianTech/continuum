# AI Provider Adapter Architecture

## 🎯 Design Goals

1. **Zero code duplication** - Providers sharing the same API format share code
2. **Trivial to add new providers** - 20-30 lines per provider
3. **Multimodal support** - Text, audio, video, images
4. **Hybrid local + cloud** - Ollama for local, APIs for large models
5. **Cost optimization** - Track usage, route by cost/latency/quality
6. **Integration testing** - Test all providers systematically
7. **🔐 Security-first** - API keys never exposed to browser, automatic redaction from logs

## 📦 Adapter Hierarchy

```
AIProviderAdapter (interface)
├── BaseLocalAdapter (local inference servers)
│   ├── OllamaAdapter ✅
│   ├── LMStudioAdapter
│   └── LlamaCppServerAdapter
│
├── BaseOpenAICompatibleAdapter (OpenAI API format)
│   ├── OpenAIAdapter ✅
│   ├── TogetherAIAdapter ✅
│   ├── FireworksAdapter ✅
│   ├── GroqAdapter
│   ├── AnyscaleAdapter
│   ├── PerplexityAdapter
│   ├── MistralAdapter
│   ├── DeepInfraAdapter
│   └── ReplicateAdapter
│
└── Proprietary Adapters (unique APIs)
    ├── AnthropicAdapter ✅ (already implemented)
    ├── GoogleGeminiAdapter
    └── CohereAdapter
```

## 🔥 Code Reuse: The Power of Base Classes

### Example: Adding Together AI (25 lines!)

```typescript
import { BaseOpenAICompatibleAdapter } from './BaseOpenAICompatibleAdapter';
import { getSecret } from '../../../../system/secrets/SecretManager';

export class TogetherAIAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey?: string) {
    super({
      providerId: 'together',
      providerName: 'Together AI',
      apiKey: apiKey || getSecret('TOGETHER_API_KEY', 'TogetherAIAdapter') || '',
      baseUrl: 'https://api.together.xyz',  // Only difference!
      defaultModel: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo',
      timeout: 60000,
      supportedCapabilities: ['text-generation', 'chat', 'embeddings'],
    });
  }
}
```

**That's it!** Inherits all functionality from `BaseOpenAICompatibleAdapter`:
- Text generation with retries
- Health checks
- Model listing
- Token counting
- Error handling
- Cost calculation (override if needed)

### Providers Using OpenAI Format (95% code sharing)

| Provider | Base URL | Added Value |
|----------|----------|-------------|
| OpenAI | `api.openai.com` | Official, DALL-E, GPT-4 |
| Together AI | `api.together.xyz` | Llama 70B+, fine-tuning |
| Fireworks | `api.fireworks.ai` | Fast inference, LoRA |
| Groq | `api.groq.com` | Ultra-fast inference (LPU) |
| Anyscale | `api.endpoints.anyscale.com` | Ray scaling |
| Perplexity | `api.perplexity.ai` | Search-augmented |
| Mistral | `api.mistral.ai` | European, Mixtral models |
| DeepInfra | `api.deepinfra.com` | Cost-effective |

## 🏗️ Implementation Status

### ✅ Completed
- [x] `AIProviderTypesV2.ts` - Multimodal type system
- [x] `BaseLocalAdapter.ts` - Local inference servers
- [x] `BaseOpenAICompatibleAdapter.ts` - OpenAI API format
- [x] `OllamaAdapter.ts` - Local Ollama models
- [x] `OpenAIAdapter.ts` - Official OpenAI
- [x] `TogetherAIAdapter.ts` - Together AI (Llama 70B+)
- [x] `FireworksAdapter.ts` - Fireworks AI

### 🚧 To Implement (Easy - 20 lines each)
- [ ] `GroqAdapter.ts` - Ultra-fast inference
- [ ] `AnyscaleAdapter.ts` - Ray-based scaling
- [ ] `PerplexityAdapter.ts` - Search-augmented generation
- [ ] `MistralAdapter.ts` - Mixtral models
- [ ] `GoogleGeminiAdapter.ts` - Proprietary API
- [ ] `CohereAdapter.ts` - Proprietary API

### 🔬 Testing Strategy

#### Unit Tests (Per Adapter)
```typescript
describe('TogetherAIAdapter', () => {
  it('should generate text', async () => {
    const adapter = new TogetherAIAdapter();
    await adapter.initialize();

    const response = await adapter.generateText({
      messages: [{ role: 'user', content: 'Hello!' }],
    });

    expect(response.text).toBeTruthy();
    expect(response.provider).toBe('together');
  });
});
```

#### Integration Tests (Multi-Provider Routing)
```typescript
describe('AIProviderDaemon', () => {
  it('should route to fastest available provider', async () => {
    // Register multiple providers with priorities
    daemon.registerAdapter(new OllamaAdapter(), { priority: 100 });
    daemon.registerAdapter(new TogetherAIAdapter(), { priority: 90 });
    daemon.registerAdapter(new OpenAIAdapter(), { priority: 80 });

    // Request should use Ollama (highest priority)
    const response = await daemon.generateText({
      messages: [{ role: 'user', content: 'Test' }],
    });

    expect(response.provider).toBe('ollama');
  });

  it('should failover to backup provider', async () => {
    // Simulate Ollama offline
    jest.spyOn(ollamaAdapter, 'healthCheck').mockResolvedValue({ status: 'unhealthy' });

    // Should automatically use Together AI
    const response = await daemon.generateText({
      messages: [{ role: 'user', content: 'Test' }],
    });

    expect(response.provider).toBe('together');
  });
});
```

## 🎯 Capability-Based Routing

The daemon can select providers based on required capabilities:

```typescript
// Need multimodal (text + image)? Use GPT-4V
const response = await daemon.generateText({
  messages: [{
    role: 'user',
    content: [
      { type: 'text', text: 'What's in this image?' },
      { type: 'image', image: { url: 'https://...' } }
    ]
  }],
  preferredCapabilities: ['multimodal']
});
// → Routes to OpenAI (GPT-4V)

// Need just text? Use local Ollama
const response = await daemon.generateText({
  messages: [{ role: 'user', content: 'Hello!' }],
  preferredCapabilities: ['text-generation']
});
// → Routes to Ollama (free, local, private)

// Need 70B+ model? Use Together AI
const response = await daemon.generateText({
  messages: [{ role: 'user', content: 'Complex reasoning task' }],
  model: 'meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo'
});
// → Routes to Together AI
```

## 💰 Cost Optimization

Track costs across all providers:

```typescript
const stats = await daemon.getUsageStats();

console.log(stats);
// {
//   ollama: { requests: 1500, cost: 0, avgLatency: 45ms },
//   together: { requests: 200, cost: $2.50, avgLatency: 350ms },
//   openai: { requests: 50, cost: $15.00, avgLatency: 800ms }
// }
```

**Smart routing strategies:**
1. **Local-first**: Try Ollama, fall back to cloud if needed
2. **Cost-optimized**: Use cheapest provider that meets quality bar
3. **Latency-optimized**: Use fastest provider (Groq for speed, Ollama for local)
4. **Quality-optimized**: Use best model regardless of cost (GPT-4, Claude 3.5)

## 🔐 Security: SecretManager Integration

**All API keys are managed securely via SecretManager:**

### Key Security Features
1. **Server-side only** - API keys NEVER sent to browser
2. **Multi-source loading** - `~/.continuum/config.env` → `process.env` → `.env`
3. **Automatic redaction** - Keys filtered from logs, screenshots, error messages
4. **Audit trail** - Track which adapters access which keys
5. **Graceful degradation** - Missing keys don't crash system

### Usage in Adapters
```typescript
import { getSecret } from '../../../../system/secrets/SecretManager';

export class OpenAIAdapter extends BaseOpenAICompatibleAdapter {
  constructor(apiKey?: string) {
    super({
      providerId: 'openai',
      providerName: 'OpenAI',
      // ✅ Secure: Uses SecretManager
      apiKey: apiKey || getSecret('OPENAI_API_KEY', 'OpenAIAdapter') || '',
      baseUrl: 'https://api.openai.com',
      // ...
    });
  }
}
```

### API Key Setup
Users configure API keys in `~/.continuum/config.env`:
```bash
OPENAI_API_KEY=sk-...
ANTHROPIC_API_KEY=sk-ant-...
TOGETHER_API_KEY=...
FIREWORKS_API_KEY=...
```

**Future**: Persona-guided widget for secure API key entry through UI.

## 🚀 Next Steps

1. **✅ SecretManager integration** - COMPLETED
2. **Implement remaining adapters** (20 lines each, trivial)
3. **Add integration tests** for multi-provider routing
4. **Implement cost tracking** and usage analytics
5. **Add streaming support** for real-time responses
6. **ProcessPool integration** for local model management
7. **LoRA adapter hot-swapping** (genome evolution)
8. **Persona-guided API key setup widget**

## 📊 M1 Mac Capabilities

### Local Inference (Ollama + MLX)
- **2B-7B models**: Run instantly, 16GB RAM
- **13B-70B models**: Quantized (4-bit), slower but works

### Training (MLX)
- **LoRA fine-tuning**: 7B-13B models
- **Full fine-tuning**: Only 2B-7B models
- **Training time**: 1-4 hours for LoRA on small datasets

### Hybrid Strategy
```
Coordination (fast, cheap)     → Ollama llama3.2:1b (local)
Chat responses (quality)       → Ollama phi3:mini or Together Llama 70B
Complex reasoning (best)       → GPT-4 or Claude 3.5 Sonnet
Multimodal (images)           → GPT-4V or Claude 3.5
Training (LoRA)               → MLX (local) or Together AI (cloud)
```

## 🎉 Summary

**Before this architecture:**
- Each provider = 300+ lines of boilerplate
- Adding providers = tedious, error-prone
- No code reuse

**After this architecture:**
- Each provider = 20-30 lines
- Adding providers = trivial
- 95% code reuse for OpenAI-compatible APIs
- Multimodal support built-in
- Cost/latency tracking unified

**Result:** Can support 10+ providers with minimal effort!
