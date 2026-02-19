# AI Provider Adapter Architecture

## 🎯 Vision: Universal AI Provider Support

Support **any AI provider** with a clean, unified interface:
- **Local Models**: Ollama, llama.cpp, vLLM, etc.
- **Cloud APIs**: OpenAI, Anthropic (Claude), Google (Gemini), DeepSeek, etc.
- **Specialized Providers**: Together.ai, Fireworks.ai, Groq, etc.
- **Custom LoRA Adapters**: Genomic layers for persona specialization

## 🏗️ Current Architecture

### Adapter Hierarchy
```
BaseAIProviderAdapter (abstract)
├── BaseLocalAdapter (local models with installation/loading)
│   ├── OllamaAdapter (✅ Implemented)
│   └── LlamaCppAdapter (✅ Implemented)
├── BaseOpenAICompatibleAdapter (OpenAI-style APIs)
│   ├── OpenAIAdapter (✅ Implemented)
│   ├── TogetherAIAdapter (✅ Implemented)
│   ├── FireworksAdapter (✅ Implemented)
│   └── GroqAdapter (📋 Future)
└── AnthropicAdapter (✅ Implemented - uses Claude SDK)
```

### Core Interfaces

```typescript
/**
 * Every adapter implements this interface
 */
interface AIProviderAdapter {
  readonly providerId: string;
  readonly providerName: string;

  // Text generation (the core operation)
  generateText(request: TextGenerationRequest): Promise<TextGenerationResponse>;

  // Health monitoring
  healthCheck(): Promise<HealthStatus>;

  // Model discovery
  getAvailableModels(): Promise<ModelInfo[]>;

  // Lifecycle
  initialize(): Promise<void>;
  shutdown(): Promise<void>;
}
```

### Unified Request/Response Types

```typescript
/**
 * Provider-agnostic text generation request
 * All adapters accept this format, translate to provider-specific API
 */
interface TextGenerationRequest {
  readonly messages: Array<{
    role: 'system' | 'user' | 'assistant';
    content: string;
    name?: string;
    images?: string[];  // Base64 or URLs for multimodal
  }>;
  readonly model?: string;
  readonly temperature?: number;
  readonly maxTokens?: number;
  readonly systemPrompt?: string;
  readonly preferredProvider?: string;
  readonly requestId?: UUID;
}

/**
 * Provider-agnostic response
 * All adapters return this format
 */
interface TextGenerationResponse {
  readonly text: string;
  readonly finishReason: 'stop' | 'length' | 'error';
  readonly model: string;
  readonly provider: string;
  readonly usage?: {
    readonly inputTokens: number;
    readonly outputTokens: number;
    readonly totalTokens: number;
  };
  readonly responseTime: number;
  readonly requestId?: UUID;
}
```

## 🔧 How to Add a New Provider

### Example: Adding DeepSeek Adapter

**Step 1: Create Adapter File**
```typescript
// daemons/ai-provider-daemon/shared/adapters/DeepSeekAdapter.ts

import { BaseOpenAICompatibleAdapter } from './BaseOpenAICompatibleAdapter';

/**
 * DeepSeek Adapter
 *
 * DeepSeek uses OpenAI-compatible API format
 * Base URL: https://api.deepseek.com/v1
 * Models: deepseek-chat, deepseek-coder
 */
export class DeepSeekAdapter extends BaseOpenAICompatibleAdapter {
  readonly providerId = 'deepseek';
  readonly providerName = 'DeepSeek';

  constructor(apiKey: string) {
    super({
      baseURL: 'https://api.deepseek.com/v1',
      apiKey: apiKey,
      defaultModel: 'deepseek-chat',
      organization: undefined
    });
  }

  async getAvailableModels(): Promise<string[]> {
    return [
      'deepseek-chat',
      'deepseek-coder',
      'deepseek-reasoner'
    ];
  }

  protected getModelCapabilities(model: string): ModelCapabilities {
    return {
      modelId: model,
      providerId: 'deepseek',
      capabilities: ['text', 'function-calling', 'streaming'],
      maxContextTokens: model.includes('coder') ? 16000 : 4096,
      supportsImages: false,
      supportsFunctionCalling: true,
      supportsStreaming: true
    };
  }
}
```

**Step 2: Register in AIProviderDaemon**
```typescript
// daemons/ai-provider-daemon/server/AIProviderDaemonServer.ts

import { DeepSeekAdapter } from '../shared/adapters/DeepSeekAdapter';

class AIProviderDaemonServer {
  async initialize() {
    // ... existing adapters ...

    // Add DeepSeek if API key provided
    const deepseekKey = process.env.DEEPSEEK_API_KEY;
    if (deepseekKey) {
      const deepseek = new DeepSeekAdapter(deepseekKey);
      await this.registerAdapter('deepseek', deepseek);
      console.log('✅ DeepSeek adapter registered');
    }
  }
}
```

**Step 3: Use It!**
```typescript
// Now any PersonaUser can use DeepSeek
const response = await aiProviderDaemon.generateText({
  messages: [{ role: 'user', content: 'Write a Python function' }],
  model: 'deepseek-coder',
  preferredProvider: 'deepseek'
});
```

### Example: Adding Google Gemini Adapter

Gemini uses a different API format, so extend the base class directly:

```typescript
// daemons/ai-provider-daemon/shared/adapters/GeminiAdapter.ts

import { BaseAIProviderAdapter } from '../BaseAIProviderAdapter';
import { GoogleGenerativeAI } from '@google/generative-ai';

/**
 * Google Gemini Adapter
 *
 * Uses Google's official SDK (@google/generative-ai)
 * Models: gemini-pro, gemini-pro-vision, gemini-1.5-pro
 */
export class GeminiAdapter extends BaseAIProviderAdapter {
  readonly providerId = 'gemini';
  readonly providerName = 'Google Gemini';

  private client: GoogleGenerativeAI;

  constructor(apiKey: string) {
    super();
    this.client = new GoogleGenerativeAI(apiKey);
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const startTime = Date.now();
    const model = this.client.getGenerativeModel({ model: request.model || 'gemini-pro' });

    // Convert our format to Gemini format
    const parts = request.messages.map(msg => ({
      text: msg.content
    }));

    try {
      const result = await model.generateContent({
        contents: [{ role: 'user', parts }],
        generationConfig: {
          temperature: request.temperature,
          maxOutputTokens: request.maxTokens
        }
      });

      const response = result.response;
      const text = response.text();

      return {
        text,
        finishReason: 'stop',
        model: request.model || 'gemini-pro',
        provider: 'gemini',
        usage: {
          inputTokens: response.usageMetadata?.promptTokenCount || 0,
          outputTokens: response.usageMetadata?.candidatesTokenCount || 0,
          totalTokens: response.usageMetadata?.totalTokenCount || 0
        },
        responseTime: Date.now() - startTime,
        requestId: request.requestId
      };
    } catch (error) {
      throw new Error(`Gemini generation failed: ${error.message}`);
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const model = this.client.getGenerativeModel({ model: 'gemini-pro' });
      const startTime = Date.now();

      await model.generateContent({ contents: [{ role: 'user', parts: [{ text: 'ping' }] }] });

      return {
        status: 'healthy',
        apiAvailable: true,
        responseTime: Date.now() - startTime,
        lastChecked: Date.now()
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: 0,
        lastChecked: Date.now(),
        message: error.message
      };
    }
  }

  async getAvailableModels(): Promise<string[]> {
    return [
      'gemini-pro',
      'gemini-pro-vision',
      'gemini-1.5-pro',
      'gemini-1.5-flash'
    ];
  }

  protected async restartProvider(): Promise<void> {
    // Cloud API - just recreate client with backoff
    await new Promise(resolve => setTimeout(resolve, 5000));
    console.log('✅ Gemini: Client reconnected after backoff');
  }
}
```

## 🧬 LoRA Adapter Integration

**CRITICAL DESIGN GOAL**: LoRA and fine-tuning should work **identically** whether using local models (Ollama) or SOTA cloud models (GPT-4, Claude). The adapter abstracts the difference.

### Phase 1: External LoRA Loading (Current Design)

LoRA adapters are loaded externally by the AI provider (Ollama, llama.cpp) and referenced by model name:

```typescript
// PersonaUser specifies LoRA-adapted model
const personaConfig = {
  providerConfig: {
    provider: 'ollama',
    model: 'llama3.2:3b-typescript-expert',  // LoRA adapter baked into model name
    temperature: 0.7
  }
};

// Ollama loads the LoRA adapter when we request this model
const response = await aiProviderDaemon.generateText({
  messages: [...],
  model: 'llama3.2:3b-typescript-expert',  // Ollama finds this in ~/.ollama/models
  preferredProvider: 'ollama'
});
```

**How to Create LoRA-Adapted Models:**
```bash
# 1. Train LoRA adapter (outside of Continuum)
# Use Unsloth, Axolotl, or other LoRA training tools

# 2. Convert to GGUF format (if needed)
python convert-lora-to-gguf.py

# 3. Register with Ollama
ollama create typescript-expert -f Modelfile

# Modelfile:
# FROM llama3.2:3b
# ADAPTER ./typescript-lora.gguf
# SYSTEM "You are a TypeScript expert"

# 4. Now use it in Continuum
# Just specify model: "typescript-expert" in PersonaUser config
```

### Cloud Provider Fine-Tuning (SOTA Models)

**The Challenge**: Cloud providers (OpenAI, Anthropic, Google) don't support dynamic LoRA loading. But they DO support fine-tuning - they just call it differently.

**The Solution**: Adapters abstract the difference. PersonaUser uses identical API, adapter translates.

#### OpenAI Fine-Tuning
```typescript
/**
 * OpenAI fine-tuning creates a new model ID
 * Adapter maps LoRA concept → fine-tuned model ID
 */

// 1. Fine-tune GPT-4 on your dataset (outside Continuum)
// Use OpenAI fine-tuning API or web interface
// This creates a new model: ft:gpt-4-0613:your-org:typescript-expert:abc123

// 2. Register with Continuum
const personaConfig = {
  providerConfig: {
    provider: 'openai',
    model: 'ft:gpt-4-0613:your-org:typescript-expert:abc123',  // Fine-tuned model ID
    temperature: 0.7
  }
};

// 3. Use it EXACTLY like Ollama LoRA
const response = await aiProviderDaemon.generateText({
  messages: [...],
  model: 'ft:gpt-4-0613:your-org:typescript-expert:abc123'
});

// PersonaUser doesn't know or care that this is a cloud fine-tune vs local LoRA!
```

#### Anthropic Fine-Tuning
```typescript
/**
 * Anthropic fine-tuning (when available) works similarly
 */
const personaConfig = {
  providerConfig: {
    provider: 'anthropic',
    model: 'claude-3-opus-typescript-expert',  // Custom fine-tuned Claude
    temperature: 0.7
  }
};
```

#### Google Gemini Fine-Tuning
```typescript
/**
 * Gemini supports fine-tuning via Vertex AI
 */
const personaConfig = {
  providerConfig: {
    provider: 'gemini',
    model: 'tunedModels/typescript-expert-abc123',  // Tuned model endpoint
    temperature: 0.7
  }
};
```

### RAG: Universal Across All Providers

**RAG works identically everywhere** - this is our fallback when fine-tuning isn't available:

```typescript
/**
 * RAG-enhanced persona (works with ANY provider)
 */
const personaConfig = {
  providerConfig: {
    provider: 'openai',  // or 'anthropic', 'gemini', 'ollama', etc.
    model: 'gpt-4-turbo',
    temperature: 0.7
  },
  ragConfig: {
    enabled: true,
    contextWindow: 10,  // Include last 10 messages
    semanticSearch: true,  // Use embedding similarity
    knowledgeBase: 'typescript-docs'  // Specialized knowledge
  }
};

// PersonaUser automatically injects RAG context into every message
// This works IDENTICALLY for GPT-4, Claude, Gemini, or Ollama
```

### Adapter Implementation: Transparent Fine-Tuning

```typescript
/**
 * Base adapter handles fine-tuning abstraction
 */
abstract class BaseAIProviderAdapter {
  /**
   * Adapters translate "lora" concept to provider-specific fine-tuning
   */
  protected resolveModel(request: TextGenerationRequest): string {
    // If model ID looks like a fine-tuned model, use it as-is
    if (this.isFineTunedModel(request.model)) {
      return request.model;
    }

    // If genomic layers specified, resolve to provider-specific format
    if (request.genomicLayers) {
      return this.resolveGenomicModel(request.genomicLayers);
    }

    // Default: use base model
    return request.model || this.defaultModel;
  }

  /**
   * Provider-specific fine-tune detection
   */
  protected abstract isFineTunedModel(modelId: string): boolean;
}

// OpenAI implementation
class OpenAIAdapter extends BaseAIProviderAdapter {
  protected isFineTunedModel(modelId: string): boolean {
    return modelId.startsWith('ft:');  // OpenAI fine-tuned models start with "ft:"
  }
}

// Ollama implementation
class OllamaAdapter extends BaseAIProviderAdapter {
  protected isFineTunedModel(modelId: string): boolean {
    // Ollama: any model name could be LoRA-adapted
    return true;  // Check if model exists in ~/.ollama/models
  }
}

// Anthropic implementation
class AnthropicAdapter extends BaseAIProviderAdapter {
  protected isFineTunedModel(modelId: string): boolean {
    return !modelId.startsWith('claude-3-');  // Non-standard names = fine-tuned
  }
}
```

### The Beautiful Symmetry

```typescript
/**
 * IDENTICAL PersonaUser configuration regardless of provider
 */

// Local Ollama with LoRA
const localPersona = {
  providerConfig: {
    provider: 'ollama',
    model: 'llama3.2:3b-typescript-expert',  // LoRA adapter
    temperature: 0.7
  }
};

// Cloud OpenAI with fine-tuning
const cloudPersona = {
  providerConfig: {
    provider: 'openai',
    model: 'ft:gpt-4:org:typescript-expert:abc',  // Fine-tuned model
    temperature: 0.7
  }
};

// PersonaUser code is IDENTICAL - adapter handles the translation
await persona.respondToMessage(message);  // Works the same either way!
```

### Phase 2: Dynamic LoRA Assembly (Future - Genomic Layers)

```typescript
/**
 * Future: Dynamic LoRA layer assembly
 *
 * PersonaUser requests capability, GenomicService assembles optimal LoRA stack
 */
interface GenomicLoRARequest {
  baseModel: string;                    // "llama3.2:3b"
  capabilities: string[];               // ["typescript", "debugging", "testing"]
  proficiencyLevel: number;             // 0.8 (80% mastery desired)
  performanceTarget: {
    accuracy: number;                   // 0.9 (90% correct solutions)
    speed: 'fast' | 'medium' | 'slow';
  };
}

// GenomicService finds optimal LoRA layers via 512-vector cosine similarity
const loraStack = await genomicService.assembleOptimalLayers({
  baseModel: 'llama3.2:3b',
  capabilities: ['typescript', 'debugging'],
  proficiencyLevel: 0.8,
  performanceTarget: { accuracy: 0.9, speed: 'fast' }
});

// Returns:
// [
//   { layerId: 'ts-syntax-v2', embedding: [...], weight: 0.6 },
//   { layerId: 'debug-patterns-v1', embedding: [...], weight: 0.4 }
// ]

// AIProviderDaemon dynamically loads these layers
await aiProviderDaemon.loadLoRAStack(loraStack);

// Now model has TypeScript + debugging expertise
const response = await aiProviderDaemon.generateText({
  messages: [{ role: 'user', content: 'Debug this TypeScript error' }],
  model: 'llama3.2:3b',
  loraStack: loraStack  // Applied dynamically
});
```

### Phase 3: Academy-Trained LoRA Layers (Future)

```typescript
/**
 * Future: AI competitions generate LoRA layers
 *
 * PersonaUsers compete in Academy, winners' adaptations become community LoRA layers
 */

// 1. Competition generates training data
const competitionResult = await academyService.runCompetition({
  challenge: 'typescript-refactoring',
  participants: [helperAI, teacherAI, codeReviewAI],
  duration: '2-hours',
  evaluationCriteria: ['correctness', 'elegance', 'performance']
});

// 2. Winner's behavioral patterns extracted as LoRA layer
const loraLayer = await genomicService.extractLoRAFromCompetition(competitionResult);
// Returns: { layerId: 'refactor-winner-2025-10-15', embedding: [...], trainingContext: {...} }

// 3. Community rates and adopts layer
await genomicService.publishLoRALayer(loraLayer);

// 4. Other PersonaUsers can now use this specialized layer
const optimizedPersona = await genomicService.assembleOptimalLayers({
  baseModel: 'llama3.2:3b',
  capabilities: ['typescript', 'refactoring'],
  includeCompetitionWinners: true  // Includes Academy-trained layers
});
```

## 🎯 Provider Selection Strategy

### Automatic Provider Selection

```typescript
/**
 * AIProviderDaemon intelligently routes to optimal provider
 */
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  // 1. User specifies preferred provider?
  if (request.preferredProvider) {
    const adapter = this.adapters.get(request.preferredProvider);
    if (adapter) return adapter.generateText(request);
  }

  // 2. Model name implies provider? (e.g., "gpt-4" → OpenAI)
  const inferredProvider = this.inferProviderFromModel(request.model);
  if (inferredProvider) {
    return inferredProvider.generateText(request);
  }

  // 3. Fall back to default provider (usually Ollama for local-first)
  return this.defaultProvider.generateText(request);
}
```

### Provider Priority Ranking

```typescript
/**
 * Priority order for provider selection:
 *
 * 1. Explicit: request.preferredProvider = 'deepseek'
 * 2. Model-specific: model = 'gpt-4' → OpenAI
 * 3. Capability-based: needsVision = true → gemini-pro-vision
 * 4. Performance-based: fastestProvider() → local Ollama
 * 5. Cost-based: cheapestProvider() → DeepSeek or Groq
 * 6. Availability: healthyProviders() → whatever works
 */
```

## 📊 Current Provider Status

| Provider | Status | Models | Use Case |
|----------|--------|--------|----------|
| **Ollama** | ✅ Implemented | llama3.2:1b, 3b, 7b, 70b | Local-first, privacy, LoRA-ready |
| **llama.cpp** | ✅ Implemented | Any GGUF model | Ultra-lightweight, mobile |
| **OpenAI** | ✅ Implemented | gpt-4, gpt-4-turbo, gpt-3.5 | Premium quality, expensive |
| **Anthropic** | ✅ Implemented | claude-3-opus, sonnet, haiku | Best reasoning, expensive |
| **Together.ai** | ✅ Implemented | mixtral, llama, qwen | Fast, cheap, diverse models |
| **Fireworks** | ✅ Implemented | mixtral, llama, code-llama | Fast inference, coding |
| **Groq** | 📋 Future | llama3, mixtral | Fastest inference (LPU) |
| **DeepSeek** | 📋 Future | deepseek-chat, coder | Cheapest tokens, good quality |
| **Google Gemini** | 📋 Future | gemini-pro, 1.5-pro | Multimodal, large context |
| **xAI Grok** | 📋 Future | grok-1, grok-2 | X/Twitter integration |
| **Mistral AI** | 📋 Future | mistral-small, medium, large | European alternative |

## 🚀 Implementation Roadmap

### Phase 1: Core Providers (COMPLETED)
- ✅ Ollama (local models)
- ✅ OpenAI (premium quality)
- ✅ Anthropic (Claude)
- ✅ Together.ai (cheap + diverse)
- ✅ Fireworks (fast inference)

### Phase 2: Expand Provider Coverage
- 📋 Groq (fastest LPU inference)
- 📋 DeepSeek (cheapest tokens)
- 📋 Google Gemini (multimodal + large context)
- 📋 Mistral AI (European alternative)
- 📋 xAI Grok (X integration)

### Phase 3: LoRA Integration
- 📋 External LoRA loading (Ollama Modelfile)
- 📋 Dynamic LoRA layer assembly (GenomicService)
- 📋 512-vector cosine similarity search
- 📋 LoRA stack caching and optimization

### Phase 4: Academy Training
- 📋 Competition-driven LoRA generation
- 📋 Behavioral pattern extraction
- 📋 Community LoRA marketplace
- 📋 Performance-based genomic optimization

## 🔧 Testing New Adapters

```bash
# Test adapter implementation
npm run test:adapter -- --provider=deepseek

# Test health monitoring
npm run test:health -- --provider=deepseek

# Test model discovery
npm run test:models -- --provider=deepseek

# Integration test with PersonaUser
npm run test:persona -- --provider=deepseek --model=deepseek-chat
```

## 📚 Related Files

- `BaseAIProviderAdapter.ts` - Abstract base with health monitoring
- `BaseOpenAICompatibleAdapter.ts` - For OpenAI-style APIs
- `BaseLocalAdapter.ts` - For local models (Ollama, llama.cpp)
- `AdapterTypes.ts` - Shared interfaces
- `AIProviderDaemonServer.ts` - Adapter registry and routing
- `PersonaUser.ts` - Consumer of adapters
- `PersonaWorkerThread.ts` - Worker thread integration

## 🌟 Design Principles

1. **Provider Agnostic**: Unified interface for all providers
2. **Health Monitoring**: Automatic recovery from failures
3. **Local First**: Prefer local models (privacy, cost, speed)
4. **Cloud Fallback**: Use cloud APIs when local unavailable
5. **Cost Optimization**: Route to cheapest provider for capability
6. **Performance Tracking**: Monitor latency, quality, cost per provider
7. **Graceful Degradation**: System works even if providers fail
8. **Easy Extension**: Adding new provider takes <100 lines of code

## 🎯 The Ultimate Goal

**Any AI provider, any model, any LoRA adapter - seamlessly integrated with zero configuration.**

PersonaUsers just specify what they need, and the system finds the optimal provider + model + LoRA combination automatically.
