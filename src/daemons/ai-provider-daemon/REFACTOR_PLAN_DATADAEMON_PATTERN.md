# AI Provider Daemon Refactor - DataDaemon Pattern
## Follow Proven shared/server/browser Architecture

**Pattern**: Copy DataDaemon's proven architecture exactly

---

## 🎯 Architecture (Following DataDaemon)

```
DataDaemon Pattern:                    AIProviderDaemon Pattern:
├── shared/                            ├── shared/
│   ├── DataDaemon.ts (85%)           │   ├── AIProviderDaemon.ts (85%)
│   ├── DataStorageAdapter.ts         │   ├── AIModelAdapter.ts
│   └── DataTypes.ts                  │   └── AIProviderTypes.ts
├── server/                            ├── server/
│   ├── DataDaemonServer.ts           │   ├── AIProviderDaemonServer.ts
│   └── SqliteStorageAdapter.ts       │   ├── OllamaModelAdapter.ts (spawn OK)
└── browser/                           │   └── DeepSeekModelAdapter.ts
    ├── DataDaemonBrowser.ts          └── browser/
    └── IndexedDBAdapter.ts               └── AIProviderDaemonBrowser.ts
```

---

## 📂 File Structure

```
daemons/ai-provider-daemon/
├── shared/
│   ├── AIProviderDaemon.ts           # 85% logic (routing, coordination)
│   ├── AIModelAdapter.ts             # Abstract adapter interface
│   ├── AIProviderTypes.ts            # Pure types (no Node.js, no browser)
│   └── AIProviderError.ts            # Error classes
│
├── server/
│   ├── AIProviderDaemonServer.ts     # Server-specific (model mgmt)
│   ├── adapters/
│   │   ├── OllamaModelAdapter.ts     # ✅ spawn() OK here
│   │   ├── DeepSeekModelAdapter.ts   # HTTP API
│   │   ├── OpenAIModelAdapter.ts     # HTTP API
│   │   └── AnthropicModelAdapter.ts  # HTTP API
│   └── ResourceManager.ts            # VRAM tracking (server-only)
│
└── browser/
    └── AIProviderDaemonBrowser.ts    # Delegates to server
```

**Total: ~15 files, all <200 lines**

---

## 🏗️ Implementation

### **1. shared/AIModelAdapter.ts** (Abstract Interface)

```typescript
/**
 * Abstract Model Adapter - Environment-agnostic interface
 * Like DataStorageAdapter - defines contract, implementations in server/
 */
export interface AIModelAdapter {
  readonly providerId: string;
  readonly providerName: string;

  /**
   * Health check (fetch API works everywhere)
   */
  healthCheck(): Promise<HealthStatus>;

  /**
   * Get capabilities (pure computation, no I/O)
   */
  getCapabilities(modelId: string): ModelCapabilities;

  /**
   * Get recommended models (pure data, no I/O)
   */
  getRecommendedModels(): ModelRecommendation[];
}

/**
 * Server Model Adapter - Server-specific operations
 * Like SqliteStorageAdapter - extends interface with server capabilities
 */
export interface ServerModelAdapter extends AIModelAdapter {
  /**
   * Check if model is installed locally
   * Server-only: checks filesystem
   */
  checkAvailability(modelId: string): Promise<boolean>;

  /**
   * Install model (download, setup)
   * Server-only: spawn, exec, fs operations
   */
  install(modelId: string, onProgress?: ProgressCallback): Promise<void>;

  /**
   * Generate text
   * Server-only: calls local model or API
   */
  generate(modelId: string, request: TextGenerationRequest): Promise<TextGenerationResponse>;
}
```

### **2. shared/AIProviderDaemon.ts** (85% Logic)

```typescript
/**
 * AI Provider Daemon - Shared Logic (85%)
 * Like DataDaemon - environment-agnostic routing and coordination
 */
export class AIProviderDaemon extends DaemonBase {
  public readonly subpath = '/ai-provider';

  protected adapters: Map<string, AIModelAdapter> = new Map();
  protected initialized = false;

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('AIProviderDaemon', context, router);
  }

  /**
   * Initialize (called by subclasses)
   */
  protected async initialize(): Promise<void> {
    console.log('🤖 AIProviderDaemon: Initializing...');
    this.initialized = true;
  }

  /**
   * Handle messages (routing logic - works everywhere)
   */
  async handleMessage(message: JTAGMessage): Promise<BaseResponsePayload> {
    const payload = message.payload as AIProviderPayload;

    switch (payload.type) {
      case 'generate-text':
        return await this.handleGenerateText(payload);
      case 'health-check':
        return await this.handleHealthCheck(payload);
      case 'list-providers':
        return await this.handleListProviders(payload);
      default:
        return createPayload(payload.context, payload.sessionId, {
          success: false,
          timestamp: new Date().toISOString(),
          error: `Unknown operation: ${(payload as any).type}`
        });
    }
  }

  /**
   * Register adapter (called by subclasses)
   */
  protected registerAdapter(adapter: AIModelAdapter, priority: number): void {
    console.log(`🔌 Registering ${adapter.providerName} (priority ${priority})`);
    this.adapters.set(adapter.providerId, adapter);
  }

  /**
   * Select best adapter (shared logic)
   */
  protected selectAdapter(preferredProvider?: string): AIModelAdapter | null {
    if (preferredProvider) {
      return this.adapters.get(preferredProvider) || null;
    }
    // Default: first registered adapter
    return Array.from(this.adapters.values())[0] || null;
  }

  /**
   * Get available providers (shared logic)
   */
  getAvailableProviders(): string[] {
    return Array.from(this.adapters.keys());
  }

  // Handle methods delegate to subclass-specific operations
  protected async handleGenerateText(payload: AIProviderPayload): Promise<BaseResponsePayload> {
    // Subclass implements actual generation
    throw new Error('handleGenerateText must be implemented by subclass');
  }

  protected async handleHealthCheck(payload: AIProviderPayload): Promise<BaseResponsePayload> {
    // Health checks work everywhere (fetch API)
    const healthMap = new Map<string, HealthStatus>();

    for (const [providerId, adapter] of this.adapters) {
      try {
        const health = await adapter.healthCheck();
        healthMap.set(providerId, health);
      } catch (error) {
        healthMap.set(providerId, {
          status: 'unhealthy',
          apiAvailable: false,
          responseTime: 0,
          lastChecked: Date.now(),
          message: error instanceof Error ? error.message : String(error)
        });
      }
    }

    return createPayload(payload.context, payload.sessionId, {
      success: true,
      timestamp: new Date().toISOString(),
      data: { providers: Array.from(healthMap.entries()) }
    });
  }

  protected async handleListProviders(payload: AIProviderPayload): Promise<BaseResponsePayload> {
    const providers = this.getAvailableProviders();
    return createPayload(payload.context, payload.sessionId, {
      success: true,
      timestamp: new Date().toISOString(),
      data: { providers, count: providers.length }
    });
  }

  /**
   * Shutdown (shared logic)
   */
  async shutdown(): Promise<void> {
    console.log('🔄 AIProviderDaemon: Shutting down...');
    this.adapters.clear();
    this.initialized = false;
    await super.shutdown();
  }

  // =============================================
  // CLEAN STATIC INTERFACE (like DataDaemon)
  // =============================================

  private static sharedInstance: AIProviderDaemon | undefined;

  static initialize(instance: AIProviderDaemon): void {
    AIProviderDaemon.sharedInstance = instance;
  }

  static async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized');
    }
    // Delegates to server instance
    return await (AIProviderDaemon.sharedInstance as any).generateText(request);
  }

  static async checkHealth(): Promise<Map<string, HealthStatus>> {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized');
    }
    // Works everywhere - just calls adapters
    const healthMap = new Map<string, HealthStatus>();
    for (const [id, adapter] of AIProviderDaemon.sharedInstance['adapters']) {
      healthMap.set(id, await adapter.healthCheck());
    }
    return healthMap;
  }

  static getProviders(): string[] {
    if (!AIProviderDaemon.sharedInstance) {
      throw new Error('AIProviderDaemon not initialized');
    }
    return AIProviderDaemon.sharedInstance.getAvailableProviders();
  }
}
```

### **3. server/AIProviderDaemonServer.ts** (Server-Specific)

```typescript
/**
 * AI Provider Daemon Server - Server-specific implementation
 * Like DataDaemonServer - adds server capabilities
 */
export class AIProviderDaemonServer extends AIProviderDaemon {
  private serverAdapters: Map<string, ServerModelAdapter> = new Map();

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);
  }

  /**
   * Server initialization - register local model adapters
   */
  protected async initialize(): Promise<void> {
    await super.initialize();

    // Register Ollama (local, free, spawn OK here)
    const ollama = new OllamaModelAdapter({ apiEndpoint: 'http://localhost:11434' });
    this.registerServerAdapter(ollama, 100);

    // Register DeepSeek (API, free tier)
    const deepseek = new DeepSeekModelAdapter({ apiKey: process.env.DEEPSEEK_API_KEY });
    this.registerServerAdapter(deepseek, 90);

    // Initialize static interface
    AIProviderDaemon.initialize(this);

    console.log(`✅ AIProviderDaemonServer: Initialized with ${this.serverAdapters.size} adapters`);
  }

  /**
   * Register server adapter
   */
  private registerServerAdapter(adapter: ServerModelAdapter, priority: number): void {
    this.serverAdapters.set(adapter.providerId, adapter);
    this.registerAdapter(adapter, priority);  // Also register in base
  }

  /**
   * Generate text (server implementation)
   */
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    const adapter = this.selectServerAdapter(request.preferredProvider);
    if (!adapter) {
      throw new Error('No suitable AI provider available');
    }

    // Check availability, auto-install if needed
    if (!await adapter.checkAvailability(request.model || 'llama3.2:3b')) {
      console.log(`📦 Installing ${request.model}...`);
      await adapter.install(request.model || 'llama3.2:3b', (progress) => {
        console.log(`   ${progress.percentComplete}%...`);
      });
    }

    // Generate
    return await adapter.generate(request.model || 'llama3.2:3b', request);
  }

  /**
   * Handle generate-text message
   */
  protected async handleGenerateText(payload: AIProviderPayload): Promise<BaseResponsePayload> {
    if (!payload.request) {
      return createPayload(payload.context, payload.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        error: 'Missing request'
      });
    }

    try {
      const response = await this.generateText(payload.request);
      return createPayload(payload.context, payload.sessionId, {
        success: true,
        timestamp: new Date().toISOString(),
        data: response
      });
    } catch (error) {
      return createPayload(payload.context, payload.sessionId, {
        success: false,
        timestamp: new Date().toISOString(),
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  /**
   * Select server adapter
   */
  private selectServerAdapter(preferredProvider?: string): ServerModelAdapter | null {
    if (preferredProvider) {
      return this.serverAdapters.get(preferredProvider) || null;
    }
    return Array.from(this.serverAdapters.values())[0] || null;
  }
}
```

### **4. server/adapters/OllamaModelAdapter.ts** (Server Adapter)

```typescript
import { spawn } from 'child_process';  // ✅ OK - server only
import type { ServerModelAdapter, TextGenerationRequest, TextGenerationResponse } from '../../shared/AIModelAdapter';

/**
 * Ollama Model Adapter - Server Implementation
 * ✅ spawn() OK here - server-only file
 */
export class OllamaModelAdapter implements ServerModelAdapter {
  readonly providerId = 'ollama';
  readonly providerName = 'Ollama';

  private config: { apiEndpoint: string };

  constructor(config: { apiEndpoint: string }) {
    this.config = config;
  }

  async healthCheck(): Promise<HealthStatus> {
    // ✅ fetch works everywhere
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
        message: String(error)
      };
    }
  }

  async checkAvailability(modelId: string): Promise<boolean> {
    // ✅ fetch works everywhere
    const response = await fetch(`${this.config.apiEndpoint}/api/tags`);
    const data = await response.json();
    return data.models.some((m: any) => m.name === modelId);
  }

  async install(modelId: string, onProgress?: ProgressCallback): Promise<void> {
    // ✅ spawn OK - server-only method
    return new Promise((resolve, reject) => {
      const process = spawn('ollama', ['pull', modelId]);

      process.stdout.on('data', (data: Buffer) => {
        const match = data.toString().match(/(\d+)%/);
        if (match && onProgress) {
          onProgress({
            model: modelId,
            status: 'downloading',
            percentComplete: parseInt(match[1]),
            bytesDownloaded: 0,
            bytesTotal: 0
          });
        }
      });

      process.on('close', (code) => {
        if (code === 0) {
          console.log(`✅ Installed ${modelId}`);
          resolve();
        } else {
          reject(new Error(`Installation failed: ${code}`));
        }
      });

      process.on('error', reject);
    });
  }

  async generate(modelId: string, request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // ✅ fetch works everywhere
    const response = await fetch(`${this.config.apiEndpoint}/api/generate`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: modelId,
        prompt: this.formatPrompt(request.messages),
        temperature: request.temperature,
        stream: false
      })
    });

    const data = await response.json();

    return {
      text: data.response,
      finishReason: data.done ? 'stop' : 'length',
      model: modelId,
      provider: this.providerId,
      responseTime: 0
    };
  }

  getCapabilities(modelId: string): ModelCapabilities {
    return {
      modelId,
      providerId: this.providerId,
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
        description: 'Fast, high-quality small model',
        size: '2GB',
        quality: 'excellent',
        speed: 'fast',
        free: true,
        requiresAPIKey: false,
        capabilities: ['text', 'streaming']
      }
    ];
  }

  private formatPrompt(messages: Array<{ role: string; content: string }>): string {
    return messages.map(m => `${m.role}: ${m.content}`).join('\n\n');
  }
}
```

---

## ✅ Migration Steps (IMMEDIATE Integration)

### **Step 1: Create Shared Types** (30 mins)
- [x] `shared/AIProviderTypes.ts` (types only, no code)
- [x] `shared/AIModelAdapter.ts` (interfaces only)

### **Step 2: Create Shared Daemon** (1 hour)
- [ ] `shared/AIProviderDaemon.ts` (85% logic, routing)
- [ ] Keep existing code as fallback

### **Step 3: Create Server Implementation** (2 hours)
- [ ] `server/AIProviderDaemonServer.ts`
- [ ] `server/adapters/OllamaModelAdapter.ts`
- [ ] Replace hardcoded `ollama pull` with adapter

### **Step 4: Update Registration** (30 mins)
- [ ] Update `server/generated.ts` to use new server class
- [ ] Deploy and test llama3.2:3b generation

### **Step 5: Add More Providers** (ongoing)
- [ ] `server/adapters/DeepSeekModelAdapter.ts`
- [ ] `server/adapters/OpenAIModelAdapter.ts`
- [ ] Each adapter is independent, zero downtime

---

## 🎯 Success Criteria

**Architecture Validation:**
```bash
# No Node.js in shared
grep -r "spawn\|exec\|require\|fs\." daemons/ai-provider-daemon/shared/
# Should return ZERO

# Server has Node.js
grep -r "spawn" daemons/ai-provider-daemon/server/adapters/
# Should find OllamaModelAdapter

# All files <200 lines
find daemons/ai-provider-daemon -name "*.ts" -exec wc -l {} \; | awk '$1 > 200'
# Should return ZERO
```

**Functional:**
- ✅ llama3.2:3b generates responses (existing behavior preserved)
- ✅ Auto-installation works (no manual `ollama pull`)
- ✅ Can add DeepSeek without touching Ollama code
- ✅ Zero downtime during refactor

---

**Ready to implement Step 1-2 NOW?**
