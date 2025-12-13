# AIProviderDaemon → AIProviderWorker Migration Map

## Current Architecture (TypeScript)

```
AIProviderDaemonServer (TypeScript)
├── Message Handler: handleMessage()
│   ├── generate-text → generateText()
│   ├── health-check → handleHealthCheck()
│   └── list-providers → handleListProviders()
│
├── Adapter Registry
│   ├── OllamaAdapter (fetch → localhost:11434)
│   ├── AnthropicAdapter (fetch → api.anthropic.com)
│   ├── OpenAIAdapter (fetch → api.openai.com)
│   ├── GrokAdapter (fetch → api.x.ai)
│   └── ... (pluggable adapters)
│
├── Features
│   ├── Rate limiting (50 req/sec, 20 concurrent)
│   ├── Health monitoring (heartbeat, failure tracking)
│   ├── Metrics (DaemonMetrics)
│   ├── ProcessPool integration (genome inference)
│   └── Request queuing (AsyncQueue)
│
└── Integration Points
    ├── DataDaemon (store AIGenerationEntity)
    ├── SecretManager (API keys)
    └── Events (system notifications)
```

## New Architecture (Daemon + Worker Pattern)

**Pattern Reference**: LoggerDaemon (working implementation)

### TypeScript Daemon (Source of Truth - Portability Layer)
```
AIProviderDaemon (extends DaemonBase)
├── Owns AIProviderWorkerClient (direct socket connection)
├── Defines protocol types (source of truth → codegen → Rust)
├── Lifecycle management (start/stop/health)
├── Feature flag (USE_RUST_AI_PROVIDER)
├── Discoverable via system/daemons
└── Graceful fallback to legacy TypeScript adapters
```

### Rust Worker (Implementation - Efficiency Layer)
```
AIProviderWorker (Rust binary, started by start-workers.sh)
├── Generated types from TypeScript (protocol.rs)
├── Connection Handler: handle_message()
│   ├── generate → generate_text()
│   ├── generate-stream → generate_text_stream()
│   ├── embeddings → generate_embeddings()
│   ├── list-providers → list_providers()
│   └── ping → health_check()
│
├── Provider Modules
│   ├── ollama.rs (ollama-rs crate)
│   ├── anthropic.rs (reqwest + streaming)
│   ├── openai.rs (reqwest + streaming)
│   ├── grok.rs (reqwest + streaming)
│   └── ... (modular providers)
│
├── Features
│   ├── Tokio async runtime
│   ├── Concurrent request handling (tokio::spawn per request)
│   ├── Rate limiting (tokio::time::sleep based)
│   ├── Health monitoring (stats tracking)
│   ├── Streaming (tokio-stream)
│   └── Logging via LoggerWorker
│
└── Process Isolation
    ├── Crashes don't affect main system
    ├── Independent testing/development
    └── Auto-restart by start-workers.sh
```

### Key Differences from Old Architecture
- ❌ **Old**: Daemon spawns child process (ProcessManager complexity)
- ✅ **New**: Daemon owns socket connection (simple, proven with Logger)
- ❌ **Old**: Manual type translation (TypeScript ↔ Rust drift possible)
- ✅ **New**: Codegen from TypeScript (absolute type cohesion)
- ❌ **Old**: Worker started by daemon (lifecycle coupling)
- ✅ **New**: Worker started by start-workers.sh (independent)

## Message Protocol Mapping

### Current (TypeScript → TypeScript)
```typescript
// Client
const response = await router.postMessage({
  endpoint: '/ai-provider',
  payload: {
    type: 'generate-text',
    request: {
      messages: [...],
      preferredProvider: 'ollama'
    }
  }
});
```

### New (TypeScript → Rust via JTAG)
```typescript
// Client
const response = await aiProviderClient.send('generate', {
  provider: 'ollama',
  messages: [...],
  stream: false
});
```

## Core Methods Migration

### 1. Text Generation

**Before (TypeScript):**
```typescript
async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
  const adapter = this.selectAdapter(request.preferredProvider);

  // ProcessPool routing or direct adapter call
  if (processPool) {
    return await processPool.executeInference(adapter, request);
  }

  return await adapter.generate(request);
}
```

**After (Rust):**
```rust
async fn handle_generate(
    request: GenerateRequest,
    providers: &ProviderRegistry
) -> Result<GenerateResponse> {
    let provider = providers.get(&request.provider)?;

    match request.provider.as_str() {
        "ollama" => ollama::generate(request, provider).await,
        "anthropic" => anthropic::generate(request, provider).await,
        "openai" => openai::generate(request, provider).await,
        "grok" => grok::generate(request, provider).await,
        _ => Err(Error::ProviderNotFound)
    }
}
```

### 2. Streaming Generation

**Before (TypeScript - doesn't exist cleanly):**
```typescript
// Current implementation doesn't stream - buffers full response
async generateText(request) {
  const response = await fetch(url, { body: JSON.stringify(request) });
  const fullText = await response.text();
  return { text: fullText };
}
```

**After (Rust):**
```rust
async fn handle_generate_stream(
    stream: &mut UnixStream,
    request: GenerateRequest,
    providers: &ProviderRegistry
) -> Result<()> {
    let provider = providers.get(&request.provider)?;

    let mut token_stream = match request.provider.as_str() {
        "ollama" => ollama::generate_stream(request, provider).await?,
        "anthropic" => anthropic::generate_stream(request, provider).await?,
        _ => return Err(Error::StreamingNotSupported)
    };

    while let Some(chunk) = token_stream.next().await {
        send_response(stream, chunk?).await?;
    }

    Ok(())
}
```

### 3. Embeddings

**Before (TypeScript):**
```typescript
async generateEmbedding(request: EmbeddingRequest): Promise<EmbeddingResponse> {
  const adapter = this.selectAdapter(request.preferredProvider);
  return await adapter.generateEmbedding(request);
}
```

**After (Rust):**
```rust
async fn handle_embeddings(
    request: EmbeddingsRequest,
    providers: &ProviderRegistry
) -> Result<EmbeddingsResponse> {
    let provider = providers.get(&request.provider)?;

    match request.provider.as_str() {
        "ollama" => ollama::embeddings(request, provider).await,
        "openai" => openai::embeddings(request, provider).await,
        _ => Err(Error::EmbeddingsNotSupported)
    }
}
```

### 4. Health Monitoring

**Before (TypeScript):**
```typescript
private healthState = {
  isHealthy: true,
  consecutiveFailures: 0,
  lastSuccessTime: Date.now(),
  lastHeartbeat: Date.now()
};

async handleHealthCheck(): Promise<HealthStatus[]> {
  const statuses = [];
  for (const [name, adapter] of this.adapters) {
    statuses.push({
      providerId: name,
      available: adapter.adapter.isHealthy(),
      latency: adapter.adapter.getLatency()
    });
  }
  return statuses;
}
```

**After (Rust):**
```rust
pub struct HealthStats {
    pub providers: HashMap<String, ProviderHealth>,
    pub uptime_ms: u64,
    pub total_requests: u64,
    pub failed_requests: u64,
    pub active_streams: usize,
}

async fn handle_ping(providers: &ProviderRegistry) -> Result<HealthStats> {
    let mut health = HealthStats::new();

    for (name, provider) in providers.iter() {
        health.providers.insert(
            name.clone(),
            provider.check_health().await?
        );
    }

    Ok(health)
}
```

## Adapter Architecture Mapping

### Current TypeScript Adapters
```typescript
interface AIProviderAdapter {
  generate(request: TextGenerationRequest): Promise<TextGenerationResponse>;
  generateEmbedding(request: EmbeddingRequest): Promise<EmbeddingResponse>;
  isHealthy(): boolean;
}

class OllamaAdapter implements AIProviderAdapter {
  async generate(request) {
    const response = await fetch('http://localhost:11434/api/generate', {
      method: 'POST',
      body: JSON.stringify({ model: request.model, prompt: request.prompt })
    });
    return await response.json();
  }
}
```

### New Rust Provider Modules
```rust
// workers/ai-provider/src/providers/ollama.rs
pub async fn generate(
    request: GenerateRequest,
    provider: &Provider
) -> Result<GenerateResponse> {
    let ollama = Ollama::default();
    let response = ollama.generate(GenerationRequest::new(
        request.model,
        request.prompt
    )).await?;

    Ok(GenerateResponse {
        text: response.response,
        model: request.model,
        tokens: response.eval_count,
    })
}

pub async fn generate_stream(
    request: GenerateRequest,
    provider: &Provider
) -> Result<impl Stream<Item = Result<String>>> {
    let ollama = Ollama::default();
    let stream = ollama.generate_stream(GenerationRequest::new(
        request.model,
        request.prompt
    )).await?;

    Ok(stream.map(|chunk| {
        chunk.map(|c| c.response)
    }))
}
```

```rust
// workers/ai-provider/src/providers/anthropic.rs
pub async fn generate(
    request: GenerateRequest,
    provider: &Provider
) -> Result<GenerateResponse> {
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &provider.api_key)
        .header("anthropic-version", "2023-06-01")
        .json(&json!({
            "model": request.model,
            "messages": request.messages,
            "max_tokens": request.max_tokens.unwrap_or(1024)
        }))
        .send()
        .await?;

    let data: AnthropicResponse = response.json().await?;
    Ok(GenerateResponse {
        text: data.content[0].text.clone(),
        model: request.model,
        tokens: data.usage.output_tokens,
    })
}
```

## Protocol-First Design (TypeScript → Rust Codegen)

### Philosophy

**TypeScript = Source of Truth**:
- Defines API contracts and protocol types
- Portability layer (works everywhere: browser, server, CLI)
- Natural companion for web standards

**Rust = Implementation**:
- Generated types from TypeScript (absolute cohesion)
- Efficiency layer (performance-critical computation)
- Natural companion for systems-level work

### Protocol Definition (TypeScript)

```typescript
// shared/ipc/ai-provider/AIProviderProtocol.ts

/** @rust-export */
export interface GenerateRequest {
  provider: string;
  model: string;
  messages: Message[];
  stream: boolean;
  maxTokens?: number;
  temperature?: number;
}

/** @rust-export */
export interface GenerateResponse {
  text: string;
  model: string;
  tokens: number;
  finishReason: string;
}

/** @rust-export */
export interface Message {
  role: 'user' | 'assistant' | 'system';
  content: string;
}
```

### Generated Rust Types (Auto-generated)

```rust
// workers/ai-provider/src/protocol.rs (GENERATED)

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GenerateRequest {
    pub provider: String,
    pub model: String,
    pub messages: Vec<Message>,
    pub stream: bool,
    pub max_tokens: Option<i32>,
    pub temperature: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GenerateResponse {
    pub text: String,
    pub model: String,
    pub tokens: i32,
    pub finish_reason: String,
}
```

**Codegen Tools**: ts-rs, typeshare, or custom script

**Benefits**:
- Single source of truth (impossible for types to drift)
- Compile-time safety on both sides
- Natural division of labor (portability vs efficiency)
- Pattern established with LoggerDaemon

---

## TypeScript Daemon Layer (Pattern from LoggerDaemon)

After migration, AIProviderDaemon becomes a DaemonBase wrapper that owns the WorkerClient:

```typescript
/**
 * AIProviderDaemon - DaemonBase wrapper that owns WorkerClient
 *
 * Pattern: LoggerDaemon reference implementation
 * - Daemon owns AIProviderWorkerClient (direct socket connection)
 * - No child process spawning (worker started by start-workers.sh)
 * - TypeScript defines protocol (source of truth)
 * - Rust implements efficiently (systems level)
 */
export class AIProviderDaemon extends DaemonBase {
  public readonly subpath = 'ai-provider';
  protected log: ComponentLogger;

  private workerClient: AIProviderWorkerClient | null = null;
  private useRustWorker = process.env.USE_RUST_AI_PROVIDER !== 'false';

  constructor(context: JTAGContext, router: JTAGRouter) {
    super('AIProviderDaemon', context, router);
    this.log = Logger.create('AIProviderDaemon', 'daemons/AIProviderDaemon');
  }

  /**
   * Initialize daemon - create direct socket connection
   * (No child process spawning!)
   */
  protected async initialize(): Promise<void> {
    this.log.info('Initializing AIProviderDaemon with Rust worker');

    // Create direct socket connection to Rust worker
    this.workerClient = new AIProviderWorkerClient({
      socketPath: '/tmp/jtag-ai-provider-worker.sock',
      timeout: 30000,
      userId: 'ai-provider-daemon'
    });

    // Connect to Rust worker (non-blocking)
    this.workerClient.connect()
      .then(() => this.log.info('Connected to Rust AIProviderWorker'))
      .catch((err) => {
        this.log.warn('Rust worker connection failed, will use fallback', err);
        this.workerClient = null;
      });

    // Keep TypeScript adapters as fallback (temporary during migration)
    await this.loadLegacyAdapters();

    this.log.info('AIProviderDaemon initialized');
  }

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // Try Rust worker first
    if (this.useRustWorker && this.workerClient) {
      try {
        return await this.workerClient.generate({
          provider: request.preferredProvider,
          messages: request.messages,
          model: request.model,
          stream: false
        });
      } catch (err) {
        this.log.error('Rust worker failed, falling back to TypeScript', err);
        // Fall through to legacy
      }
    }

    // Fallback to TypeScript adapter
    return await this.legacyGenerateText(request);
  }

  async *streamText(request: TextGenerationRequest): AsyncIterableIterator<string> {
    if (this.useRustWorker && this.workerClient) {
      try {
        yield* this.workerClient.generateStream({
          provider: request.preferredProvider,
          messages: request.messages,
          model: request.model
        });
        return;
      } catch (err) {
        this.log.error('Rust streaming failed', err);
        // Fallback to non-streaming
      }
    }

    // Fallback: buffer full response (no streaming)
    const response = await this.legacyGenerateText(request);
    yield response.text;
  }

  /**
   * Cleanup daemon - close socket connection
   */
  async cleanup(): Promise<void> {
    this.log.info('Shutting down AIProviderDaemon');

    if (this.workerClient) {
      await this.workerClient.disconnect();
      this.workerClient = null;
    }

    this.log.info('AIProviderDaemon shutdown complete');
  }
}
```

**Key Pattern Benefits**:
- Simple: Daemon owns connection, no child process complexity
- Lifecycle: DaemonBase provides start/stop/restart/health
- Discoverable: Registered in system/daemons
- Fallback: Graceful degradation if worker unavailable
- Proven: LoggerDaemon is working implementation

## Migration Checklist

### Phase 0: Protocol Definition (TypeScript → Rust Codegen)
- [ ] Define protocol types in TypeScript (`shared/ipc/ai-provider/AIProviderProtocol.ts`)
- [ ] Set up codegen tooling (ts-rs, typeshare, or custom)
- [ ] Generate Rust types (`workers/ai-provider/src/protocol.rs`)
- [ ] Integrate codegen into build process (`npm run build:protocols`)
- [ ] Verify type cohesion (compile both sides)

### Phase 1: Rust Worker (Isolated)
- [ ] Create `workers/ai-provider/` structure
- [ ] Use generated protocol types (no manual translation!)
- [ ] Implement Ollama provider (ollama-rs)
- [ ] Implement Anthropic provider (reqwest)
- [ ] Implement OpenAI provider (reqwest)
- [ ] Implement streaming for all providers
- [ ] Implement embeddings
- [ ] Implement health monitoring
- [ ] Write standalone tests

### Phase 2: TypeScript Client + Daemon Integration
- [ ] Create `AIProviderWorkerClient.ts` (similar to LoggerWorkerClient)
- [ ] Implement `generate()` method
- [ ] Implement `generateStream()` AsyncIterator
- [ ] Implement `embeddings()` method
- [ ] Implement `ping()` health check
- [ ] Write standalone IPC tests
- [ ] Refactor AIProviderDaemon to own WorkerClient (no ProcessManager!)
- [ ] Remove child process spawning (daemon owns connection directly)
- [ ] Add feature flag: `USE_RUST_AI_PROVIDER`
- [ ] Implement fallback logic (Rust → TypeScript legacy adapters)

### Phase 3: Testing & Shadow Mode
- [ ] Write integration tests (daemon → worker)
- [ ] Add shadow testing (send to both Rust and TS, compare results)
- [ ] Deploy with flag OFF (worker runs but not used)
- [ ] Monitor for discrepancies

### Phase 4: Gradual Rollout
- [ ] Enable Ollama via flag (local, safest)
- [ ] Monitor for issues
- [ ] Enable Anthropic via flag
- [ ] Enable OpenAI via flag
- [ ] Enable all remaining providers

### Phase 5: Cleanup
- [ ] Remove TypeScript adapter code
- [ ] Make Rust worker required dependency
- [ ] Update documentation
- [ ] Archive old adapters

## Risk Mitigation

### Risk 1: All AI Stops Working
**Mitigation:** Feature flag + fallback ensures TypeScript stays working

### Risk 2: Streaming Breaks Existing Code
**Mitigation:** Non-streaming mode available, gradual rollout

### Risk 3: API Key Access from Rust
**Mitigation:** Pass keys via JTAG protocol, SecretManager stays in TypeScript

### Risk 4: Performance Regression
**Mitigation:** Shadow testing compares latency before full cutover

### Risk 5: Rust Worker Crashes
**Mitigation:** Process isolation + auto-restart via modular launcher

## Performance Expectations

### Current (TypeScript)
- Request latency: ~50-100ms (Node.js fetch overhead)
- Concurrent limit: 20 requests (semaphore)
- Streaming: No (buffer full response)
- Memory: High (full responses buffered)

### Expected (Rust)
- Request latency: ~10-20ms (native reqwest)
- Concurrent limit: Unlimited (tokio spawn per request)
- Streaming: Yes (real-time tokens)
- Memory: Low (streaming chunks)

**Expected improvement: 2-5x latency reduction, unlimited concurrency, real-time streaming**

## Next Steps

1. **Start Phase 1** - Build Rust worker in isolation
2. **Test standalone** - Verify all providers work
3. **Build TypeScript client** - IPC integration
4. **Deploy with flag OFF** - Validate in production without risk
5. **Shadow test** - Compare Rust vs TypeScript results
6. **Gradual rollout** - One provider at a time
7. **Full cutover** - Remove TypeScript fallback

Ready to start Phase 1?
