# AI Provider Daemon Architecture

## Overview

The AI Provider Daemon provides a unified interface for all AI model integrations, supporting both commercial APIs (OpenAI, Anthropic) and free local models (Ollama, LM Studio, etc.). The system prioritizes **free, private, local models** while maintaining compatibility with cloud providers.

## Current State (v1.0.2509)

**Working:**
- ✅ AIProviderDaemon with adapter registry
- ✅ OllamaAdapter for local Llama models
- ✅ Text generation with llama3.2:3b (tested, working perfectly)
- ✅ Health monitoring
- ✅ Static interface (AIProviderDaemon.generateText())

**Issues:**
- ❌ Manual model installation required (`ollama pull llama3.2:3b`)
- ❌ No automatic model availability detection
- ❌ No adapter self-installation
- ❌ Single-process blocks on slow inference
- ❌ Only supports Ollama (no other free model providers)

## Target Architecture (v2.0)

### 1. Self-Installing Adapters

**Problem:** Users must manually install models before using them.

**Solution:** Adapters handle their own model lifecycle.

```typescript
interface AIProviderAdapter {
  // ... existing methods ...

  /**
   * Check if required models are installed
   * Returns list of missing models
   */
  checkModelAvailability(requiredModels: string[]): Promise<string[]>;

  /**
   * Install a model (with progress callback)
   * Adapters can use: ollama pull, lmstudio download, etc.
   */
  installModel(model: string, onProgress?: (progress: InstallProgress) => void): Promise<void>;

  /**
   * Get recommended models for this adapter
   * Allows adapters to suggest free, high-quality models
   */
  getRecommendedModels(): ModelRecommendation[];
}

interface ModelRecommendation {
  modelId: string;
  name: string;
  description: string;
  size: string;  // "2GB", "7GB", etc.
  quality: 'excellent' | 'good' | 'fair';
  speed: 'fast' | 'medium' | 'slow';
  free: boolean;
  requiresAPIKey: boolean;
}

interface InstallProgress {
  model: string;
  status: 'downloading' | 'installing' | 'verifying' | 'complete' | 'failed';
  bytesDownloaded: number;
  bytesTotal: number;
  percentComplete: number;
  estimatedTimeRemaining?: number;
}
```

**Ollama Example:**
```typescript
// OllamaAdapter.ts
async checkModelAvailability(requiredModels: string[]): Promise<string[]> {
  const availableModels = await this.getAvailableModels();
  return requiredModels.filter(m => !availableModels.includes(m));
}

async installModel(model: string, onProgress?: (progress: InstallProgress) => void): Promise<void> {
  // Execute: ollama pull llama3.2:3b
  // Stream progress to onProgress callback
  // Throw error if installation fails
}

getRecommendedModels(): ModelRecommendation[] {
  return [
    {
      modelId: 'llama3.2:3b',
      name: 'Llama 3.2 3B',
      description: 'Meta\'s latest small model - excellent for chat, 3B parameters',
      size: '2GB',
      quality: 'excellent',
      speed: 'fast',
      free: true,
      requiresAPIKey: false
    },
    {
      modelId: 'phi3:mini',
      name: 'Phi-3 Mini',
      description: 'Microsoft\'s efficient 3.8B model - great for coding',
      size: '2.3GB',
      quality: 'good',
      speed: 'fast',
      free: true,
      requiresAPIKey: false
    },
    // ... more recommendations
  ];
}
```

**Usage Flow:**
```typescript
// 1. PersonaUser requests generation with model
const response = await AIProviderDaemon.generateText({
  model: 'llama3.2:3b',
  preferredProvider: 'ollama',
  messages: [...]
});

// 2. AIProviderDaemon checks if model is available
const adapter = this.selectAdapter('ollama');
const missingModels = await adapter.checkModelAvailability(['llama3.2:3b']);

// 3. If missing, auto-install (with user consent in settings)
if (missingModels.length > 0 && this.config.autoInstallModels) {
  console.log(`📦 Installing missing model: ${missingModels[0]}`);
  await adapter.installModel(missingModels[0], (progress) => {
    console.log(`   ${progress.percentComplete}% complete...`);
  });
}

// 4. Generate text with now-available model
```

### 2. Multi-Process AI Daemon (Performance)

**Problem:** Long-running inference (5-10 seconds) blocks the main process.

**Solution:** Isolate each AI provider in a separate worker thread/process.

```
┌─────────────────────────────────────────────────┐
│         AIProviderDaemon (Main Process)         │
│                                                 │
│  - Adapter registry                             │
│  - Request routing                              │
│  - Health monitoring                            │
│  - Model installation orchestration             │
└────────────┬────────────────────────────────────┘
             │
             ├─────────────┬─────────────┬─────────────┐
             │             │             │             │
     ┌───────▼──────┐ ┌───▼──────┐ ┌────▼─────┐ ┌────▼─────┐
     │   Ollama     │ │  OpenAI  │ │ Anthropic│ │ LM Studio│
     │   Worker     │ │  Worker  │ │  Worker  │ │  Worker  │
     │              │ │          │ │          │ │          │
     │ llama3.2:3b  │ │ gpt-4o   │ │ claude-3 │ │ mistral  │
     │ phi3:mini    │ │ gpt-4o-  │ │ sonnet   │ │ -7b      │
     │              │ │ mini     │ │          │ │          │
     └──────────────┘ └──────────┘ └──────────┘ └──────────┘
        (Local)        (API Key)    (API Key)    (Local)
        (Free)         (Paid)       (Paid)       (Free)
```

**Implementation:**
```typescript
// system/ai-workers/OllamaWorker.ts
import { Worker } from 'worker_threads';

export class OllamaWorker {
  private worker: Worker;
  private adapter: OllamaAdapter;

  constructor() {
    // Spawn worker thread with OllamaAdapter
    this.worker = new Worker('./ollama-worker-thread.js');
    this.adapter = new OllamaAdapter();
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // Send request to worker thread (non-blocking)
    return new Promise((resolve, reject) => {
      this.worker.postMessage({ type: 'generate', request });
      this.worker.once('message', (response) => {
        if (response.success) resolve(response.data);
        else reject(new Error(response.error));
      });
    });
  }
}

// AIProviderDaemon.ts
private workers: Map<string, AIWorker> = new Map();

private async initialize(): Promise<void> {
  // Spawn worker for each adapter
  this.workers.set('ollama', new OllamaWorker());
  this.workers.set('openai', new OpenAIWorker());
  // ... etc
}

private async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  const worker = this.workers.get(request.preferredProvider);
  // This no longer blocks - worker handles inference in separate thread
  return await worker.generateText(request);
}
```

**Benefits:**
- Main process never blocks on slow inference
- Multiple personas can generate responses simultaneously
- Isolates provider-specific crashes (Ollama hang doesn't crash system)
- Better resource utilization on multi-core systems

### 3. Free Model Provider Ecosystem

**Target Providers:**
1. **Ollama** (priority 100) - ✅ Implemented
   - llama3.2:1b, llama3.2:3b, llama3.1:7b (tested, working)
   - phi3:mini, phi3:medium
   - mistral:7b, mixtral:8x7b
   - Free, local, private

2. **LM Studio** (priority 90) - 🔄 TODO
   - Similar to Ollama but different API
   - Better GPU utilization
   - Free, local, private

3. **llama.cpp Server** (priority 80) - 🔄 TODO
   - Raw llama.cpp with HTTP server
   - Maximum performance
   - Free, local, private

4. **Hugging Face Inference API** (priority 70) - 🔄 TODO
   - Free tier: 30k tokens/month
   - Thousands of models available
   - Requires API key (free tier)

5. **OpenAI** (priority 50) - 🔄 TODO
   - gpt-4o, gpt-4o-mini
   - Paid, requires API key

6. **Anthropic** (priority 50) - 🔄 TODO
   - claude-3.5-sonnet, claude-3-opus
   - Paid, requires API key

**Adapter Priority System:**
```typescript
// Higher priority = try first
private selectAdapter(preferredProvider?: string): AIProviderAdapter {
  if (preferredProvider) {
    // User explicitly requested provider
    return this.adapters.get(preferredProvider);
  }

  // Otherwise, use priority:
  // 1. Free local providers (Ollama, LM Studio) - priority 100-80
  // 2. Free cloud providers (HuggingFace free tier) - priority 70
  // 3. Paid cloud providers (OpenAI, Anthropic) - priority 50

  return Array.from(this.adapters.values())
    .filter(a => a.enabled && a.healthy)
    .sort((a, b) => b.priority - a.priority)[0];
}
```

## Implementation Plan

### Phase 1: Model Installation (Week 1)
- [ ] Add `checkModelAvailability()` to AIProviderAdapter interface
- [ ] Add `installModel()` to AIProviderAdapter interface
- [ ] Add `getRecommendedModels()` to AIProviderAdapter interface
- [ ] Implement in OllamaAdapter
- [ ] Add auto-installation logic to AIProviderDaemon
- [ ] Add progress logging for model downloads
- [ ] Test with llama3.2:3b installation

### Phase 2: Multi-Process Architecture (Week 2)
- [ ] Design worker thread architecture
- [ ] Create BaseAIWorker class
- [ ] Implement OllamaWorker
- [ ] Update AIProviderDaemon to use workers
- [ ] Test concurrent persona generation
- [ ] Verify main process never blocks

### Phase 3: Additional Free Providers (Week 3)
- [ ] Implement LM Studio adapter
- [ ] Implement llama.cpp server adapter
- [ ] Implement Hugging Face adapter
- [ ] Add provider discovery (auto-detect running services)
- [ ] Add provider recommendations UI

### Phase 4: Paid Providers (Week 4)
- [ ] Implement OpenAI adapter
- [ ] Implement Anthropic adapter
- [ ] Add API key management
- [ ] Add cost tracking
- [ ] Add usage limits/warnings

## Configuration

**User Settings (system/user/config/UserCapabilities.ts):**
```typescript
interface AICapabilities {
  // Model preferences
  preferredProvider?: 'ollama' | 'lmstudio' | 'openai' | 'anthropic' | 'auto';
  preferredModel?: string;

  // Installation settings
  autoInstallModels: boolean;  // Auto-install missing free models?
  maxModelSize: number;        // Max download size in bytes (default: 10GB)

  // Performance settings
  useWorkerThreads: boolean;   // Isolate providers in workers?
  maxConcurrentRequests: number; // Max simultaneous AI requests

  // Privacy settings
  allowCloudProviders: boolean;  // Allow OpenAI, Anthropic, etc.?
  allowTelemetry: boolean;       // Share usage stats with providers?
}
```

## Success Criteria

**Phase 1 (Model Installation):**
- ✅ User types in chat, persona with missing model auto-installs it
- ✅ Progress logging shows download status
- ✅ Persona responds after model installation completes
- ✅ No manual `ollama pull` required

**Phase 2 (Multi-Process):**
- ✅ 3 personas can generate responses simultaneously without blocking
- ✅ Main process remains responsive during 10-second inference
- ✅ Ollama crash doesn't crash entire system

**Phase 3 (Free Providers):**
- ✅ System auto-detects Ollama, LM Studio, llama.cpp if running
- ✅ User can switch between providers without code changes
- ✅ Recommendations show best free models for different tasks

**Phase 4 (Paid Providers):**
- ✅ OpenAI and Anthropic work seamlessly alongside free providers
- ✅ Cost tracking prevents surprise bills
- ✅ Graceful fallback from paid → free when limits reached

## Testing Strategy

**Model Installation Tests:**
```bash
# 1. Verify model auto-installation
./jtag data/truncate --collection=chat_messages
./jtag exec --code="/* send chat message */"
# Expect: Logs show "📦 Installing missing model: llama3.2:3b"
# Expect: Progress logs appear
# Expect: Persona responds after installation

# 2. Verify installation skip for existing models
./jtag exec --code="/* send another message */"
# Expect: No installation logs
# Expect: Immediate response
```

**Multi-Process Tests:**
```bash
# 1. Concurrent generation test
./jtag exec --code="/* send 3 messages simultaneously */"
# Expect: All 3 personas respond in parallel
# Expect: Total time ~5-7 seconds (not 15-21 seconds sequential)

# 2. Main process responsiveness test
./jtag exec --code="/* send message requiring 10s inference */"
./jtag ping
# Expect: ping responds immediately even during inference
```

**Provider Tests:**
```bash
# 1. Provider auto-detection
./jtag ai/list-providers
# Expect: Shows all running providers (Ollama, LM Studio, etc.)

# 2. Provider switching
./jtag ai/generate --provider=ollama --model=llama3.2:3b --prompt="Hello"
./jtag ai/generate --provider=lmstudio --model=mistral-7b --prompt="Hello"
# Expect: Both work seamlessly
```

## Open Questions

1. **Model Storage Location:** Where should downloaded models live?
   - Ollama: `~/.ollama/models/`
   - LM Studio: `~/.cache/lm-studio/`
   - Continuum: `.continuum/models/`?

2. **Model Version Management:** How to handle model updates?
   - Auto-update to latest versions?
   - Pin specific versions?
   - User choice?

3. **Disk Space Management:** How to prevent filling user's disk?
   - Max total model cache size?
   - Auto-delete unused models?
   - Warn before large downloads?

4. **Network Failures:** How to handle interrupted downloads?
   - Resume from partial download?
   - Retry logic?
   - Fallback to alternative providers?

5. **Multi-User Systems:** How to share models across users?
   - System-wide model cache?
   - Per-user isolation?
   - Symlinks?

## Related Documents

- [PersonaUser.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/system/user/shared/PersonaUser.ts) - AI persona implementation
- [ChatRAGBuilder.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/system/rag/builders/ChatRAGBuilder.ts) - RAG context building
- [AIProviderTypes.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/daemons/ai-provider-daemon/shared/AIProviderTypes.ts) - Type definitions
- [OllamaAdapter.ts](/Volumes/FlashGordon/cambrian/continuum/src/debug/jtag/daemons/ai-provider-daemon/shared/OllamaAdapter.ts) - Reference adapter implementation

## Changelog

- **2025-10-06**: Initial architecture document
  - Defined self-installing adapter interface
  - Designed multi-process worker architecture
  - Outlined free provider ecosystem
  - Created 4-phase implementation plan
