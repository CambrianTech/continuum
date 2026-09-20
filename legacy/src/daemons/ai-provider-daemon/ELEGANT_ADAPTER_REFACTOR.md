# Elegant AI Provider Adapter Refactor
## Zero-Downtime Migration to Small, Generic, Rust-Like Architecture

**Status**: 📋 Planning Phase
**Priority**: 🔥 Critical (foundations for genomic system)
**Approach**: Zero-downtime compatibility wrapper pattern

---

## 🎯 Goals

1. **Small Files**: Each file <200 lines, single responsibility
2. **True Generics**: `BaseModelAdapter<TModel, TConfig, TResponse>`
3. **No God Objects**: Separate concerns into adapters
4. **Zero Downtime**: Compatibility wrapper preserves existing behavior
5. **Free Models First**: Ollama (llama3.2:3b working), DeepSeek, Qwen

---

## 📂 New File Structure

```
daemons/ai-provider-daemon/
├── shared/
│   ├── adapters/
│   │   ├── base/
│   │   │   ├── BaseModelAdapter.ts           # <T, C, R> generic base (100 lines)
│   │   │   ├── BaseResourceAdapter.ts        # VRAM/RAM management (80 lines)
│   │   │   ├── BaseLoRAAdapter.ts            # LoRA lifecycle (90 lines)
│   │   │   ├── BaseCheckpointAdapter.ts      # State persistence (70 lines)
│   │   │   └── AdapterTypes.ts               # Shared interfaces (150 lines)
│   │   │
│   │   ├── providers/
│   │   │   ├── ollama/
│   │   │   │   ├── OllamaModelAdapter.ts     # Extends BaseModelAdapter (120 lines)
│   │   │   │   ├── OllamaResourceAdapter.ts  # Ollama-specific resources (60 lines)
│   │   │   │   ├── OllamaLoRAAdapter.ts      # Ollama LoRA support (80 lines)
│   │   │   │   └── OllamaTypes.ts            # Ollama-specific types (50 lines)
│   │   │   │
│   │   │   ├── deepseek/
│   │   │   │   ├── DeepSeekModelAdapter.ts   # API-based, no local install (100 lines)
│   │   │   │   ├── DeepSeekResourceAdapter.ts # Cloud resources (40 lines)
│   │   │   │   └── DeepSeekTypes.ts          # API types (30 lines)
│   │   │   │
│   │   │   ├── openai/
│   │   │   │   ├── OpenAIModelAdapter.ts     # API wrapper (90 lines)
│   │   │   │   ├── OpenAIResourceAdapter.ts  # Token tracking (50 lines)
│   │   │   │   └── OpenAITypes.ts            # OpenAI types (40 lines)
│   │   │   │
│   │   │   ├── anthropic/
│   │   │   │   ├── AnthropicModelAdapter.ts  # Claude adapter (95 lines)
│   │   │   │   ├── AnthropicResourceAdapter.ts # Token tracking (50 lines)
│   │   │   │   └── AnthropicTypes.ts         # Claude types (40 lines)
│   │   │   │
│   │   │   └── huggingface/
│   │   │       ├── HuggingFaceBaseAdapter.ts # Shared HF logic (110 lines)
│   │   │       ├── HFLlamaAdapter.ts         # Llama models (60 lines)
│   │   │       ├── HFQwenAdapter.ts          # Qwen models (60 lines)
│   │   │       └── HuggingFaceTypes.ts       # HF types (50 lines)
│   │   │
│   │   └── registry/
│   │       ├── AdapterRegistry.ts            # Provider discovery (120 lines)
│   │       ├── CapabilityDetector.ts         # Auto-detect capabilities (80 lines)
│   │       └── RegistryTypes.ts              # Registry types (40 lines)
│   │
│   ├── AIProviderDaemon.ts                   # THIN coordinator (150 lines)
│   └── AIProviderTypes.ts                    # Public API types (100 lines)
│
├── server/
│   └── AIProviderDaemonServer.ts             # Server registration (50 lines)
│
└── ARCHITECTURE.md                           # Updated architecture
```

**Total: ~2500 lines across 30+ small files (average 80 lines/file)**

---

## 🏗️ Core Abstractions

### **1. BaseModelAdapter<TModel, TConfig, TResponse>**

```typescript
// shared/adapters/base/BaseModelAdapter.ts (~100 lines)
import type { ModelCapability } from './AdapterTypes';

/**
 * Base model adapter - ALL providers extend this
 * Defines lifecycle: check → install → load → generate → unload
 */
export abstract class BaseModelAdapter<
  TModel,           // Model identifier type (string for Ollama, {id, version} for HF)
  TConfig,          // Provider-specific config
  TResponse         // Provider-specific response format
> {
  protected config: TConfig;

  constructor(config: TConfig) {
    this.config = config;
  }

  // ========================================
  // LIFECYCLE (all providers must implement)
  // ========================================

  /**
   * Check if model is available locally
   */
  abstract checkAvailability(model: TModel): Promise<boolean>;

  /**
   * Install model (download, setup, etc.)
   * Returns void - progress via callback
   */
  abstract install(
    model: TModel,
    onProgress?: (progress: InstallProgress) => void
  ): Promise<void>;

  /**
   * Load model into memory/VRAM
   * Returns handle for inference
   */
  abstract load(model: TModel): Promise<LoadedModelHandle>;

  /**
   * Unload model from memory/VRAM
   */
  abstract unload(handle: LoadedModelHandle): Promise<void>;

  /**
   * Health check - is provider responsive?
   */
  abstract healthCheck(): Promise<HealthStatus>;

  // ========================================
  // INFERENCE (all providers must implement)
  // ========================================

  /**
   * Generate text (blocking)
   */
  abstract generate(
    handle: LoadedModelHandle,
    request: TextGenerationRequest
  ): Promise<TResponse>;

  /**
   * Stream text generation (optional, default throws)
   */
  async *stream(
    handle: LoadedModelHandle,
    request: TextGenerationRequest
  ): AsyncGenerator<string> {
    throw new Error(`${this.getProviderId()} does not support streaming`);
  }

  // ========================================
  // CAPABILITIES (all providers must implement)
  // ========================================

  /**
   * Provider identifier (ollama, openai, anthropic, etc.)
   */
  abstract getProviderId(): string;

  /**
   * What can this model do?
   */
  abstract getCapabilities(model: TModel): ModelCapabilities;

  /**
   * Get recommended free models
   */
  abstract getRecommendedModels(): ModelRecommendation[];

  // ========================================
  // HELPERS (providers can override)
  // ========================================

  /**
   * Estimate VRAM usage (default: 0 for API providers)
   */
  estimateVRAM(model: TModel): number {
    return 0;  // Cloud providers use 0 VRAM
  }

  /**
   * Estimate download size (default: 0 for API providers)
   */
  estimateDownloadSize(model: TModel): number {
    return 0;
  }
}
```

### **2. BaseResourceAdapter (Separate Concern)**

```typescript
// shared/adapters/base/BaseResourceAdapter.ts (~80 lines)

/**
 * Resource management - VRAM/RAM tracking and eviction
 * Separate from model adapter (single responsibility)
 */
export abstract class BaseResourceAdapter {
  /**
   * Get current resource usage
   */
  abstract getCurrentUsage(): ResourceUsage;

  /**
   * Can this model fit in available resources?
   */
  abstract canFit(modelId: string, estimatedVRAM: number): boolean;

  /**
   * Evict models to free resources (LRU strategy)
   */
  abstract evict(neededVRAM: number): Promise<EvictedModel[]>;

  /**
   * Track model load
   */
  abstract trackLoad(modelId: string, vramUsed: number): void;

  /**
   * Track model unload
   */
  abstract trackUnload(modelId: string): void;
}

/**
 * Default LRU resource manager (works for all providers)
 */
export class LRUResourceAdapter extends BaseResourceAdapter {
  private vramLimit: number;
  private vramUsed: number = 0;
  private loadedModels: LRUCache<string, LoadedModelInfo>;

  constructor(vramLimit: number = 8000) {
    super();
    this.vramLimit = vramLimit;
    this.loadedModels = new LRUCache({ max: 10 });
  }

  getCurrentUsage(): ResourceUsage {
    return {
      vramUsed: this.vramUsed,
      vramLimit: this.vramLimit,
      loadedModels: this.loadedModels.size
    };
  }

  canFit(modelId: string, estimatedVRAM: number): boolean {
    return (this.vramUsed + estimatedVRAM) <= this.vramLimit;
  }

  async evict(neededVRAM: number): Promise<EvictedModel[]> {
    const evicted: EvictedModel[] = [];
    let freed = 0;

    // Evict LRU until we have space
    for (const [modelId, info] of this.loadedModels.rvalues()) {
      if (freed >= neededVRAM) break;

      evicted.push({ modelId, vramFreed: info.vramSize });
      freed += info.vramSize;
      this.vramUsed -= info.vramSize;
      this.loadedModels.delete(modelId);
    }

    return evicted;
  }

  trackLoad(modelId: string, vramUsed: number): void {
    this.loadedModels.set(modelId, { vramSize: vramUsed, loadedAt: Date.now() });
    this.vramUsed += vramUsed;
  }

  trackUnload(modelId: string): void {
    const info = this.loadedModels.get(modelId);
    if (info) {
      this.vramUsed -= info.vramSize;
      this.loadedModels.delete(modelId);
    }
  }
}
```

### **3. Ollama Adapter (Example Implementation)**

```typescript
// shared/adapters/providers/ollama/OllamaModelAdapter.ts (~120 lines)
import { BaseModelAdapter } from '../../base/BaseModelAdapter';
import type { OllamaConfig, OllamaResponse } from './OllamaTypes';

/**
 * Ollama adapter - local LLM server
 * Supports: llama3.2 (1b, 3b, 11b, 90b), phi3, mistral, qwen, etc.
 */
export class OllamaModelAdapter extends BaseModelAdapter<string, OllamaConfig, OllamaResponse> {
  getProviderId(): string {
    return 'ollama';
  }

  async checkAvailability(model: string): Promise<boolean> {
    const models = await this.listModels();
    return models.includes(model);
  }

  async install(model: string, onProgress?: (progress: InstallProgress) => void): Promise<void> {
    // Execute: ollama pull <model>
    // Stream progress to callback
    const process = spawn('ollama', ['pull', model]);

    process.stdout.on('data', (data: Buffer) => {
      const match = data.toString().match(/(\d+)%/);
      if (match && onProgress) {
        onProgress({
          model,
          status: 'downloading',
          percentComplete: parseInt(match[1]),
          bytesDownloaded: 0,  // Ollama doesn't provide this
          bytesTotal: 0
        });
      }
    });

    return new Promise((resolve, reject) => {
      process.on('close', (code) => {
        if (code === 0) resolve();
        else reject(new Error(`Ollama pull failed: ${code}`));
      });
    });
  }

  async load(model: string): Promise<LoadedModelHandle> {
    // Ollama loads on first use, just verify availability
    if (!await this.checkAvailability(model)) {
      throw new Error(`Model ${model} not available`);
    }

    return {
      modelId: model,
      providerId: this.getProviderId(),
      loadedAt: Date.now()
    };
  }

  async unload(handle: LoadedModelHandle): Promise<void> {
    // Ollama manages its own memory, no explicit unload
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const response = await fetch(`${this.config.apiEndpoint}/api/tags`, {
        signal: AbortSignal.timeout(5000)
      });

      return {
        status: response.ok ? 'healthy' : 'unhealthy',
        apiAvailable: response.ok,
        responseTime: 0,
        lastChecked: Date.now()
      };
    } catch (error) {
      return {
        status: 'unhealthy',
        apiAvailable: false,
        responseTime: 0,
        lastChecked: Date.now(),
        message: `Ollama not available: ${error instanceof Error ? error.message : String(error)}`
      };
    }
  }

  async generate(
    handle: LoadedModelHandle,
    request: TextGenerationRequest
  ): Promise<OllamaResponse> {
    const response = await fetch(`${this.config.apiEndpoint}/api/generate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: handle.modelId,
        prompt: this.formatPrompt(request.messages),
        temperature: request.temperature,
        max_tokens: request.maxTokens,
        stream: false
      })
    });

    if (!response.ok) {
      throw new Error(`Ollama generation failed: ${response.statusText}`);
    }

    return await response.json() as OllamaResponse;
  }

  getCapabilities(model: string): ModelCapabilities {
    // Vision models
    if (model.includes('llava') || model.includes('vision')) {
      return {
        modelId: model,
        providerId: 'ollama',
        capabilities: ['text', 'vision', 'multimodal'],
        maxContextTokens: 128000,
        supportsImages: true,
        supportsFunctionCalling: false,
        supportsStreaming: true
      };
    }

    // Standard text models
    return {
      modelId: model,
      providerId: 'ollama',
      capabilities: ['text', 'streaming'],
      maxContextTokens: 128000,
      supportsImages: false,
      supportsFunctionCalling: false,
      supportsStreaming: true
    };
  }

  getRecommendedModels(): ModelRecommendation[] {
    return [
      {
        modelId: 'llama3.2:3b',
        name: 'Llama 3.2 3B',
        description: 'Meta\'s latest small model - excellent for chat',
        size: '2GB',
        quality: 'excellent',
        speed: 'fast',
        free: true,
        requiresAPIKey: false
      },
      {
        modelId: 'phi3:mini',
        name: 'Phi-3 Mini',
        description: 'Microsoft\'s efficient model - great for coding',
        size: '2.3GB',
        quality: 'good',
        speed: 'fast',
        free: true,
        requiresAPIKey: false
      },
      {
        modelId: 'qwen2.5:3b',
        name: 'Qwen 2.5 3B',
        description: 'Alibaba\'s multilingual model',
        size: '2.2GB',
        quality: 'good',
        speed: 'fast',
        free: true,
        requiresAPIKey: false
      }
    ];
  }

  estimateVRAM(model: string): number {
    // Rough estimates based on model size
    if (model.includes('1b')) return 1500;   // 1.5GB
    if (model.includes('3b')) return 2500;   // 2.5GB
    if (model.includes('7b')) return 7000;   // 7GB
    if (model.includes('11b')) return 11000; // 11GB
    return 3000;  // Default 3GB
  }

  private formatPrompt(messages: Array<{role: string; content: string}>): string {
    // Convert chat format to Ollama prompt
    return messages.map(m => `${m.role}: ${m.content}`).join('\n\n');
  }

  private async listModels(): Promise<string[]> {
    const response = await fetch(`${this.config.apiEndpoint}/api/tags`);
    const data = await response.json();
    return data.models.map((m: any) => m.name);
  }
}
```

---

## 🎯 Zero-Downtime Migration Strategy

### **Phase 0: Compatibility Wrapper (Week 0 - 2 days)**
**Goal**: Preserve existing behavior, enable parallel development

```typescript
// shared/AIProviderDaemon.ts (compatibility wrapper)
export class AIProviderDaemon extends DaemonBase {
  // NEW: Adapter registry
  private adapterRegistry: AdapterRegistry;
  private resourceManager: LRUResourceAdapter;

  // OLD: Existing implementation (preserved)
  private adapters: Map<string, ProviderRegistration> = new Map();

  /**
   * Route to new or old implementation based on provider
   */
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const useNewAdapter = this.shouldUseNewAdapter(request.preferredProvider);

    if (useNewAdapter) {
      // NEW PATH: Use elegant adapter system
      return await this.generateTextNew(request);
    } else {
      // OLD PATH: Use existing OllamaAdapter
      return await this.generateTextOld(request);
    }
  }

  private shouldUseNewAdapter(provider?: string): boolean {
    // Start with false (old system), flip to true as adapters are ready
    return false;  // Week 0: All use old system
    // return provider === 'ollama-v2';  // Week 1: New Ollama only
    // return true;  // Week 2: All use new system
  }
}
```

### **Phase 1: Ollama Refactor (Week 1 - 3 days)**
**Goal**: Get llama3.2:3b working through new adapter

**Tasks**:
- [ ] Implement `BaseModelAdapter<T, C, R>`
- [ ] Implement `OllamaModelAdapter`
- [ ] Implement `LRUResourceAdapter`
- [ ] Add `ollama-v2` provider to registry
- [ ] Test llama3.2:3b generation
- [ ] Flip `shouldUseNewAdapter('ollama-v2')` to true

**Success Criteria**:
- ✅ PersonaUser can use 'ollama-v2' provider
- ✅ llama3.2:3b generates correct responses
- ✅ Old 'ollama' provider still works (compatibility)
- ✅ Zero downtime during deployment

### **Phase 2: Free Models (Week 2 - 4 days)**
**Goal**: Add DeepSeek, Qwen, more free options

**Tasks**:
- [ ] Implement `DeepSeekModelAdapter` (API-based, free tier)
- [ ] Implement `HuggingFaceBaseAdapter`
- [ ] Implement `HFQwenAdapter` extends HuggingFaceBaseAdapter
- [ ] Add to registry with priority (free > paid)
- [ ] Test each model

**Models to Support**:
1. **DeepSeek-R1** (Free API, 7B distilled model)
   - API: `https://api.deepseek.com/v1/chat/completions`
   - Free tier: 10M tokens/month
   - Quality: Excellent reasoning

2. **Qwen 2.5** (via Ollama or HuggingFace)
   - Ollama: `qwen2.5:3b`
   - Quality: Good multilingual

3. **Mistral 7B** (via Ollama)
   - Ollama: `mistral:7b`
   - Quality: Excellent general purpose

### **Phase 3: Cloud Providers (Week 3 - 3 days)**
**Goal**: Add OpenAI, Anthropic with same interface

**Tasks**:
- [ ] Implement `OpenAIModelAdapter`
- [ ] Implement `AnthropicModelAdapter`
- [ ] Add API key management
- [ ] Add cost tracking
- [ ] Lower priority than free models

### **Phase 4: Deprecate Old System (Week 4 - 2 days)**
**Goal**: Remove compatibility wrapper, clean up

**Tasks**:
- [ ] Flip all providers to new system
- [ ] Remove old `ProviderRegistration` code
- [ ] Remove compatibility wrapper
- [ ] Update PersonaUser to use new API
- [ ] Verify zero regressions

---

## 📋 File-by-File Implementation Order

**Day 1:**
1. `AdapterTypes.ts` (interfaces)
2. `BaseModelAdapter.ts` (abstract base)
3. `BaseResourceAdapter.ts` + `LRUResourceAdapter.ts`

**Day 2:**
4. `OllamaTypes.ts`
5. `OllamaModelAdapter.ts`
6. `AdapterRegistry.ts`
7. Add compatibility wrapper to `AIProviderDaemon.ts`

**Day 3:**
8. Test llama3.2:3b through new adapter
9. Deploy with `shouldUseNewAdapter('ollama-v2') = true`

**Week 2:**
10-15. DeepSeek, Qwen, Mistral adapters

**Week 3:**
16-18. OpenAI, Anthropic adapters

**Week 4:**
19. Remove old code, clean up

---

## ✅ Architecture Validation

**Rust-Like Principles:**
- ✅ Generics: `BaseModelAdapter<T, C, R>`
- ✅ No `any` types
- ✅ Small files (<200 lines each)
- ✅ Single responsibility
- ✅ Abstract base classes
- ✅ Environment-agnostic `/shared`

**Zero Downtime:**
- ✅ Compatibility wrapper preserves old behavior
- ✅ Gradual migration (one provider at a time)
- ✅ Existing PersonaUser code works unchanged
- ✅ Can rollback at any phase

**Success Test:**
```bash
# No god objects
find daemons/ai-provider-daemon -name "*.ts" -exec wc -l {} \; | awk '$1 > 200'
# Should return ZERO files

# No specific entity references in adapters
grep -r "UserEntity\|ChatMessageEntity" daemons/ai-provider-daemon/shared/adapters/
# Should return ZERO results

# Generic patterns only
grep -r "extends Base" daemons/ai-provider-daemon/shared/adapters/providers/
# Should return ALL provider adapters
```

---

## 🚀 Ready to Start?

**Next Step**: Implement Phase 0 (Compatibility Wrapper) - 2 days

**Files to Create First**:
1. `shared/adapters/base/AdapterTypes.ts`
2. `shared/adapters/base/BaseModelAdapter.ts`
3. `shared/adapters/base/BaseResourceAdapter.ts`

This gives us the foundation to build on.
