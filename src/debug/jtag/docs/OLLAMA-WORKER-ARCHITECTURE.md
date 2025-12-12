# OllamaWorker Architecture

## Vision

OllamaWorker is a high-performance Rust worker that handles all Ollama API communication for the JTAG system. It provides streaming inference, function calling, embeddings, and model management through a clean IPC interface.

## Core Requirements

1. **Streaming Inference**: Real-time token streaming for responsive AI interactions
2. **Function Calling**: Tool use support for PersonaUser autonomous agents
3. **Async/Concurrent**: Handle multiple inference requests simultaneously
4. **Model Management**: Dynamic model loading/unloading
5. **Health Monitoring**: Track Ollama service status and performance
6. **Embeddings**: Generate embeddings for RAG (Retrieval-Augmented Generation)

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                     TypeScript Layer                         │
│  ┌──────────────────┐      ┌──────────────────┐            │
│  │  PersonaUser     │      │ AIProviderDaemon │            │
│  │  (autonomous)    │──────│  (orchestration) │            │
│  └──────────────────┘      └────────┬─────────┘            │
│                                      │                       │
│                            ┌─────────▼──────────┐           │
│                            │ OllamaWorkerClient │           │
│                            │  (TypeScript IPC)  │           │
│                            └─────────┬──────────┘           │
└──────────────────────────────────────┼──────────────────────┘
                                       │ Unix Socket
                                       │ (JTAG Protocol)
┌──────────────────────────────────────▼──────────────────────┐
│                      Rust Layer                              │
│  ┌────────────────────────────────────────────────────┐     │
│  │              OllamaWorker                          │     │
│  │  ┌──────────────────────────────────────────────┐ │     │
│  │  │          connection_handler.rs               │ │     │
│  │  │  (Routes JTAG messages to handlers)          │ │     │
│  │  └──────────────────┬───────────────────────────┘ │     │
│  │                     │                              │     │
│  │  ┌──────────────────▼───────────────────────────┐ │     │
│  │  │   inference.rs (ollama-rs streaming)         │ │     │
│  │  │   - generate_stream()                        │ │     │
│  │  │   - chat_stream()                            │ │     │
│  │  │   - chat_with_tools()                        │ │     │
│  │  └──────────────────────────────────────────────┘ │     │
│  │                                                    │     │
│  │  ┌──────────────────────────────────────────────┐ │     │
│  │  │   models.rs (model management)               │ │     │
│  │  │   - list_models()                            │ │     │
│  │  │   - pull_model()                             │ │     │
│  │  │   - show_model()                             │ │     │
│  │  └──────────────────────────────────────────────┘ │     │
│  │                                                    │     │
│  │  ┌──────────────────────────────────────────────┐ │     │
│  │  │   embeddings.rs (RAG support)                │ │     │
│  │  │   - generate_embeddings()                    │ │     │
│  │  └──────────────────────────────────────────────┘ │     │
│  │                                                    │     │
│  │  ┌──────────────────────────────────────────────┐ │     │
│  │  │   health.rs (monitoring)                     │ │     │
│  │  │   - ping()                                   │ │     │
│  │  │   - stats tracking                           │ │     │
│  │  └──────────────────────────────────────────────┘ │     │
│  └────────────────────────────────────────────────────┘     │
│                                                              │
│  ┌────────────────────────────────────────────────────┐     │
│  │            ollama-rs (crate)                       │     │
│  │  - Ollama API client (v0.3.3)                     │     │
│  │  - Tokio async runtime                            │     │
│  │  - Streaming via tokio-stream                     │     │
│  └────────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────────┘
                       │
                       ▼
              ┌────────────────┐
              │ Ollama Service │
              │ (localhost:11434)
              └────────────────┘
```

## Message Protocol (JTAG)

### Request Types

All requests follow `JTAGRequest<T>` format from `jtag-protocol`:

#### 1. `generate` - Streaming Generation
```typescript
{
  type: "generate",
  payload: {
    model: "llama3.2:latest",
    prompt: "Why is the sky blue?",
    options?: {
      temperature?: number,
      top_p?: number,
      max_tokens?: number
    }
  }
}
```

**Response**: Stream of `JTAGResponse<GenerateChunk>` with same request ID:
```typescript
{ success: true, payload: { text: "The", done: false } }
{ success: true, payload: { text: " sky", done: false } }
{ success: true, payload: { text: " is", done: false } }
...
{ success: true, payload: { text: ".", done: true, stats: {...} } }
```

#### 2. `chat` - Chat with History
```typescript
{
  type: "chat",
  payload: {
    model: "llama3.2:latest",
    messages: [
      { role: "user", content: "What is Rust?" },
      { role: "assistant", content: "Rust is a systems programming language..." },
      { role: "user", content: "Why use it?" }
    ]
  }
}
```

**Response**: Streaming chunks like `generate`

#### 3. `chat-with-tools` - Function Calling
```typescript
{
  type: "chat-with-tools",
  payload: {
    model: "llama3.2:latest",
    messages: [...],
    tools: [
      {
        name: "get_weather",
        description: "Get weather for a location",
        parameters: {
          type: "object",
          properties: {
            location: { type: "string" }
          }
        }
      }
    ]
  }
}
```

**Response**:
```typescript
{
  success: true,
  payload: {
    toolCalls: [
      { name: "get_weather", arguments: { location: "San Francisco" } }
    ]
  }
}
```

#### 4. `embeddings` - Generate Embeddings
```typescript
{
  type: "embeddings",
  payload: {
    model: "nomic-embed-text",
    text: "This is the text to embed"
  }
}
```

**Response**:
```typescript
{
  success: true,
  payload: {
    embeddings: [0.123, -0.456, ...], // 768-dim vector
    model: "nomic-embed-text"
  }
}
```

#### 5. `list-models` - List Available Models
```typescript
{ type: "list-models", payload: {} }
```

**Response**:
```typescript
{
  success: true,
  payload: {
    models: [
      { name: "llama3.2:latest", size: 4700000000, modified: "..." },
      { name: "nomic-embed-text", size: 274000000, modified: "..." }
    ]
  }
}
```

#### 6. `pull-model` - Download Model
```typescript
{
  type: "pull-model",
  payload: { name: "llama3.2:latest" }
}
```

**Response**: Streaming progress updates

#### 7. `show-model` - Model Details
```typescript
{
  type: "show-model",
  payload: { name: "llama3.2:latest" }
}
```

**Response**: Model info (parameters, family, quantization, etc.)

#### 8. `ping` - Health Check
```typescript
{ type: "ping", payload: {} }
```

**Response**:
```typescript
{
  success: true,
  payload: {
    ollamaConnected: true,
    uptimeMs: 123456,
    activeRequests: 2,
    totalRequests: 147,
    totalTokens: 15234
  }
}
```

## Rust Module Structure

```
workers/ollama/
├── Cargo.toml
└── src/
    ├── main.rs                  # Entry point, socket listener
    ├── connection_handler.rs    # Route JTAG messages
    ├── inference.rs             # Generation/chat streaming
    ├── models.rs                # Model management
    ├── embeddings.rs            # Embedding generation
    ├── health.rs                # Health monitoring
    └── messages.rs              # Protocol types
```

### Cargo.toml
```toml
[package]
name = "ollama-worker"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "ollama-worker"
path = "src/main.rs"

[dependencies]
ollama-rs = "0.3.3"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
chrono = "0.4"

[dependencies.jtag-protocol]
path = "../shared"

[dependencies.logger-client]
path = "../shared"
```

## TypeScript Client Interface

```typescript
import { WorkerClient } from '../WorkerClient';

export interface GenerateOptions {
  temperature?: number;
  top_p?: number;
  max_tokens?: number;
}

export interface ChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface Tool {
  name: string;
  description: string;
  parameters: Record<string, any>;
}

export class OllamaWorkerClient extends WorkerClient {
  constructor(socketPath = '/tmp/jtag-ollama-worker.sock') {
    super({ socketPath });
  }

  /**
   * Generate streaming completion
   */
  async *generateStream(
    prompt: string,
    model: string,
    options?: GenerateOptions
  ): AsyncIterableIterator<string> {
    // Yields tokens as they arrive
  }

  /**
   * Chat with history (streaming)
   */
  async *chatStream(
    messages: ChatMessage[],
    model: string,
    options?: GenerateOptions
  ): AsyncIterableIterator<string> {
    // Yields tokens as they arrive
  }

  /**
   * Chat with function calling (non-streaming, waits for tool call decision)
   */
  async chatWithTools(
    messages: ChatMessage[],
    tools: Tool[],
    model: string
  ): Promise<ToolCallResponse> {
    // Returns tool calls to execute
  }

  /**
   * Generate embeddings for RAG
   */
  async embeddings(text: string, model: string = 'nomic-embed-text'): Promise<number[]> {
    // Returns embedding vector
  }

  /**
   * List available models
   */
  async listModels(): Promise<ModelInfo[]> {
    // Returns model list
  }

  /**
   * Pull/download a model
   */
  async pullModel(name: string): Promise<void> {
    // Downloads model with progress
  }

  /**
   * Get model details
   */
  async showModel(name: string): Promise<ModelDetails> {
    // Returns model metadata
  }

  /**
   * Health check
   */
  async ping(): Promise<HealthStatus> {
    // Returns Ollama connection status and stats
  }
}

// Singleton for app-wide use
export namespace OllamaWorkerClient {
  let instance: OllamaWorkerClient | null = null;

  export function initialize(socketPath?: string): OllamaWorkerClient {
    if (instance) throw new Error('Already initialized');
    instance = new OllamaWorkerClient(socketPath);
    return instance;
  }

  export function getInstance(): OllamaWorkerClient {
    if (!instance) throw new Error('Not initialized');
    return instance;
  }
}
```

## Streaming Protocol Implementation

### Challenge: Unix Socket Streaming

Unix domain sockets are bidirectional but not naturally suited for "push" streaming. We need a protocol:

**Option A: Multiple Responses (Recommended)**
- Send `JTAGRequest` with unique ID
- Rust immediately sends ACK: `JTAGResponse { success: true, payload: { streaming: true } }`
- Rust sends stream chunks as separate `JTAGResponse` messages with same request ID
- TypeScript client correlates by request ID
- Last chunk has `done: true` flag

**Option B: Single Response with Embedded Stream**
- Return single response with embedded newline-delimited JSON chunks
- More complex parsing

**We'll use Option A** - cleaner, fits JTAG protocol better.

### TypeScript Client Implementation

```typescript
async *generateStream(prompt: string, model: string): AsyncIterableIterator<string> {
  const requestId = crypto.randomUUID();

  // Send request
  await this.sendRaw({
    id: requestId,
    type: 'generate',
    timestamp: new Date().toISOString(),
    payload: { model, prompt }
  });

  // Listen for responses with this request ID
  for await (const response of this.listenForStream(requestId)) {
    if (response.payload.done) break;
    yield response.payload.text;
  }
}
```

### Rust Connection Handler Implementation

```rust
async fn handle_generate_stream(
    stream: &mut UnixStream,
    request: JTAGRequest<GeneratePayload>,
    ollama: &Ollama,
) -> Result<()> {
    // Send immediate ACK
    send_response(stream, JTAGResponse::success(
        request.id.clone(),
        "generate".into(),
        serde_json::json!({ "streaming": true })
    )).await?;

    // Stream tokens
    let mut token_stream = ollama
        .generate_stream(GenerationRequest::new(
            request.payload.model,
            request.payload.prompt
        ))
        .await?;

    while let Some(chunk) = token_stream.next().await {
        let chunk = chunk?;

        send_response(stream, JTAGResponse::success(
            request.id.clone(),
            "generate".into(),
            GenerateChunk {
                text: chunk.response,
                done: chunk.done,
                stats: if chunk.done { Some(chunk.stats) } else { None }
            }
        )).await?;
    }

    Ok(())
}
```

## Integration with AIProviderDaemon

AIProviderDaemon currently handles Ollama directly. We'll refactor:

**Before:**
```typescript
class AIProviderDaemon {
  private async callOllama(prompt: string): Promise<string> {
    // Direct fetch() to localhost:11434
  }
}
```

**After:**
```typescript
class AIProviderDaemon {
  private ollamaWorker: OllamaWorkerClient;

  async initialize() {
    this.ollamaWorker = OllamaWorkerClient.getInstance();
    await this.ollamaWorker.connect();
  }

  private async *streamInference(
    messages: ChatMessage[],
    persona: PersonaUser
  ): AsyncIterableIterator<string> {
    // Use OllamaWorker instead of direct API call
    yield* this.ollamaWorker.chatStream(
      messages,
      persona.model || 'llama3.2:latest'
    );
  }
}
```

## PersonaUser Integration

PersonaUser autonomous loop needs streaming inference:

```typescript
class PersonaUser extends AIUser {
  async processTask(task: Task): Promise<void> {
    // Build messages from task context
    const messages = this.buildMessages(task);

    // Stream response from Ollama via OllamaWorker
    const stream = this.aiProvider.chatStream(messages, this.model);

    let fullResponse = '';
    for await (const chunk of stream) {
      fullResponse += chunk;
      // Could emit partial updates here for real-time UI
    }

    // Process complete response
    await this.handleResponse(fullResponse, task);
  }
}
```

## Error Handling Strategy

### 1. Ollama Service Down
```typescript
{
  success: false,
  error: "Ollama service not reachable at localhost:11434",
  errorType: "ServiceUnavailable",
  payload: { suggestion: "Run: ollama serve" }
}
```

### 2. Model Not Found
```typescript
{
  success: false,
  error: "Model 'llama99' not found",
  errorType: "ModelNotFound",
  payload: {
    suggestion: "Run: ollama pull llama3.2:latest",
    availableModels: ["llama3.2:latest", "mistral:latest"]
  }
}
```

### 3. Context Length Exceeded
```typescript
{
  success: false,
  error: "Context length exceeded (8192 tokens)",
  errorType: "ContextLengthExceeded",
  payload: {
    maxTokens: 8192,
    providedTokens: 9500,
    suggestion: "Truncate conversation history"
  }
}
```

### 4. Stream Interrupted
- Rust worker logs error via LoggerWorker
- Sends error response to client
- Client can retry with exponential backoff

## Performance Considerations

### Concurrency
- Ollama handles concurrent requests natively
- OllamaWorker can spawn tokio tasks per request
- Multiple PersonaUser agents can share one OllamaWorker

### Memory
- Streaming keeps memory low (no buffering full responses)
- ollama-rs uses tokio-stream for efficient async iteration

### Latency
- Unix socket IPC: ~0.1ms overhead
- Ollama local inference: depends on model (typically 20-100 tokens/sec)
- Streaming provides immediate first-token response

## Health Monitoring

Track these metrics in `health.rs`:

```rust
pub struct HealthStats {
    pub ollama_connected: bool,
    pub uptime_ms: u64,
    pub active_requests: usize,
    pub total_requests: u64,
    pub total_tokens: u64,
    pub average_tokens_per_sec: f64,
    pub last_error: Option<String>,
}
```

Expose via `ping` command for monitoring.

## Testing Strategy

### Unit Tests (Rust)
- Test message parsing (`messages.rs`)
- Test protocol serialization
- Mock ollama-rs for inference tests

### Integration Tests (TypeScript)
```typescript
describe('OllamaWorker', () => {
  it('should stream tokens from generation', async () => {
    const client = new OllamaWorkerClient();
    await client.connect();

    const chunks: string[] = [];
    for await (const chunk of client.generateStream(
      'Say hello',
      'llama3.2:latest'
    )) {
      chunks.push(chunk);
    }

    expect(chunks.length).toBeGreaterThan(0);
    expect(chunks.join('')).toContain('hello');
  });

  it('should handle function calling', async () => {
    const client = new OllamaWorkerClient();
    await client.connect();

    const result = await client.chatWithTools(
      [{ role: 'user', content: 'What is the weather in SF?' }],
      [{ name: 'get_weather', description: '...', parameters: {...} }],
      'llama3.2:latest'
    );

    expect(result.toolCalls[0].name).toBe('get_weather');
  });
});
```

### End-to-End Tests
- Spin up OllamaWorker
- Send requests from PersonaUser
- Verify streaming works
- Verify tool calling works
- Verify error handling

## Deployment

### Development
```bash
npm run worker:start  # Auto-discovers and starts OllamaWorker
```

### Production
- OllamaWorker starts automatically with modular launcher
- Depends on LoggerWorker (for logging)
- No dependencies on other workers
- Ollama service must be running (localhost:11434)

## Migration Path

### Phase 1: Build OllamaWorker (This Session)
1. Create `workers/ollama/` directory
2. Implement Rust worker with ollama-rs
3. Implement TypeScript client
4. Test basic streaming and function calling

### Phase 2: Integrate with AIProviderDaemon
1. Refactor AIProviderDaemon to use OllamaWorkerClient
2. Remove direct Ollama HTTP calls
3. Test with existing PersonaUser tests

### Phase 3: Enable PersonaUser
1. Connect PersonaUser to OllamaWorker
2. Test autonomous loop with real streaming
3. Verify tool calling works end-to-end

### Phase 4: Optimize
1. Add connection pooling if needed
2. Tune streaming chunk sizes
3. Add request queuing/prioritization
4. Monitor performance metrics

## Success Criteria

✅ OllamaWorker builds and starts automatically
✅ Streaming inference works (tokens arrive in real-time)
✅ Function calling works (tools can be invoked)
✅ Embeddings work (RAG ready)
✅ Model management works (list/pull/show)
✅ Health monitoring works (ping returns stats)
✅ PersonaUser can use OllamaWorker for autonomous loop
✅ Multiple concurrent requests work
✅ Error handling is clear and actionable
✅ Performance is better than current TypeScript implementation

## Future Enhancements

- **Model caching**: Keep hot models in memory
- **Request batching**: Batch multiple prompts for efficiency
- **Adaptive context**: Auto-truncate when hitting context limits
- **Multi-model**: Support multiple Ollama instances
- **Quantization control**: Select quantization level per request
- **Token counting**: Track token usage per persona

---

**Next Step**: Implement Phase 1 - Build the OllamaWorker!
