# AIProviderWorker - Complete Architecture & Implementation Guide

**Version**: 1.0
**Status**: Ready for Implementation
**Migration**: AIProviderDaemon (TypeScript) → AIProviderWorker (Rust)

---

## Table of Contents

1. [Vision & Goals](#vision--goals)
2. [Architecture Overview](#architecture-overview)
3. [Migration Strategy](#migration-strategy)
4. [Testing Strategy](#testing-strategy)
5. [Implementation Plan](#implementation-plan)
6. [Module Specifications](#module-specifications)
7. [Deployment Guide](#deployment-guide)

---

## Vision & Goals

### Primary Goal
Move all AI provider logic from TypeScript (AIProviderDaemon) to Rust (AIProviderWorker) for:
- **Performance**: 2-5x latency reduction via native async
- **Isolation**: AI crashes don't kill main system
- **Streaming**: Real-time token streaming (currently buffered)
- **Concurrency**: Unlimited concurrent requests (currently limited to 20)
- **LoRA-ready**: Foundation for GPU-accelerated fine-tuning

### Current State (TypeScript)
```
AIProviderDaemonServer.ts (1000+ lines)
├── Adapter registry (Ollama, Claude, OpenAI, Grok, XAI)
├── Rate limiting (50 req/sec, 20 concurrent)
├── Health monitoring (heartbeat, failure tracking)
├── ProcessPool integration
└── Direct fetch() to APIs (no streaming)
```

### Target State (Rust + TypeScript Thin Layer)
```
AIProviderWorker (Rust)
├── Native async with tokio
├── Real streaming (tokio-stream)
├── All provider implementations
├── Process isolation
└── IPC via JTAG protocol

AIProviderDaemon (TypeScript - thin routing)
├── Feature flag (Rust or fallback)
├── AIProviderWorkerClient (IPC)
└── Graceful fallback to legacy
```

---

## Architecture Overview

### System Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                     TypeScript Layer                         │
│  ┌──────────────────┐      ┌──────────────────┐            │
│  │  PersonaUser     │      │ AIProviderDaemon │            │
│  │  (autonomous)    │──────│  (thin router)   │            │
│  └──────────────────┘      └────────┬─────────┘            │
│                                      │                       │
│                            ┌─────────▼──────────┐           │
│                            │ AIProviderWorkerClient          │
│                            │  (TypeScript IPC)  │           │
│                            └─────────┬──────────┘           │
└──────────────────────────────────────┼──────────────────────┘
                                       │ Unix Socket
                                       │ (JTAG Protocol)
┌──────────────────────────────────────▼──────────────────────┐
│                      Rust Layer                              │
│  ┌────────────────────────────────────────────────────┐     │
│  │              AIProviderWorker                      │     │
│  │  ┌──────────────────────────────────────────────┐ │     │
│  │  │          connection_handler.rs               │ │     │
│  │  │  (Routes JTAG messages)                      │ │     │
│  │  └──────────────────┬───────────────────────────┘ │     │
│  │                     │                              │     │
│  │  ┌──────────────────▼───────────────────────────┐ │     │
│  │  │   providers/                                 │ │     │
│  │  │   ├── ollama.rs (ollama-rs crate)           │ │     │
│  │  │   ├── anthropic.rs (reqwest + streaming)    │ │     │
│  │  │   ├── openai.rs (reqwest + streaming)       │ │     │
│  │  │   ├── grok.rs (reqwest)                     │ │     │
│  │  │   └── xai.rs (reqwest)                      │ │     │
│  │  └──────────────────────────────────────────────┘ │     │
│  │                                                    │     │
│  │  ┌──────────────────────────────────────────────┐ │     │
│  │  │   health.rs (monitoring + stats)             │ │     │
│  │  └──────────────────────────────────────────────┘ │     │
│  └────────────────────────────────────────────────────┘     │
│                                                              │
│  ┌────────────────────────────────────────────────────┐     │
│  │     External Crates                                │     │
│  │  ├── ollama-rs (v0.3.3)                           │     │
│  │  ├── reqwest (HTTP client)                        │     │
│  │  ├── tokio (async runtime)                        │     │
│  │  ├── tokio-stream (streaming)                     │     │
│  │  └── serde/serde_json (serialization)             │     │
│  └────────────────────────────────────────────────────┘     │
└──────────────────────────────────────────────────────────────┘
                       │
                       ▼
         ┌──────────────────────────────┐
         │  External AI Services        │
         │  ├── Ollama (localhost:11434)│
         │  ├── Claude (api.anthropic)  │
         │  ├── OpenAI (api.openai)     │
         │  ├── Grok (api.x.ai)         │
         │  └── XAI (api.x.ai)          │
         └──────────────────────────────┘
```

### Module Structure

```
workers/ai-provider/
├── Cargo.toml
├── src/
│   ├── main.rs                      # Entry point, socket listener
│   ├── connection_handler.rs        # Route JTAG messages
│   ├── providers/
│   │   ├── mod.rs                   # Provider registry
│   │   ├── ollama.rs                # Ollama via ollama-rs
│   │   ├── anthropic.rs             # Claude via reqwest
│   │   ├── openai.rs                # GPT via reqwest
│   │   ├── grok.rs                  # Grok via reqwest
│   │   └── xai.rs                   # XAI via reqwest
│   ├── health.rs                    # Health monitoring
│   └── messages.rs                  # Protocol types
└── tests/
    ├── unit/
    │   ├── ollama_tests.rs
    │   ├── anthropic_tests.rs
    │   └── messages_tests.rs
    ├── integration/
    │   ├── ollama_integration_test.rs
    │   ├── streaming_test.rs
    │   └── all_providers_test.rs
    └── standalone/
        └── test_client.ts           # Standalone IPC test
```

### Cargo.toml

```toml
[package]
name = "ai-provider-worker"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "ai-provider-worker"
path = "src/main.rs"

[dependencies]
# Ollama client
ollama-rs = "0.3.3"

# HTTP client for external APIs
reqwest = { version = "0.11", features = ["json", "stream"] }

# Async runtime
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Utilities
chrono = "0.4"
anyhow = "1"

# Shared protocol
[dependencies.jtag-protocol]
path = "../shared"

# Logger integration
[dependencies.logger-client]
path = "../shared"

[dev-dependencies]
tokio-test = "0.4"
```

---

## Migration Strategy

### Zero-Downtime Migration Path

```
Phase 1: Build in Isolation (Week 1)
├── Create workers/ai-provider/
├── Implement all providers
├── Write unit + integration tests
└── Test standalone (no system integration)

Phase 2: Parallel Deployment (Week 2)
├── Create AIProviderWorkerClient (TypeScript)
├── Add feature flag to AIProviderDaemon
├── Deploy with flag OFF (worker runs but not used)
└── Shadow test (send to both, compare results)

Phase 3: Gradual Rollout (Week 3)
├── Enable Ollama only (local, safest)
├── Monitor for issues
├── Enable external APIs one by one
└── Instant rollback capability via flag

Phase 4: Full Cutover (Week 4)
├── All providers via Rust
├── Remove TypeScript fallback code
└── AIProviderDaemon becomes pure routing layer
```

### TypeScript Thin Layer (Post-Migration)

```typescript
export class AIProviderDaemon extends DaemonBase {
  private workerClient: AIProviderWorkerClient;
  private useRustWorker = process.env.USE_RUST_AI_PROVIDER !== 'false';

  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // Try Rust worker first
    if (this.useRustWorker && this.workerClient.isConnected()) {
      try {
        return await this.workerClient.generate({
          provider: request.preferredProvider,
          messages: request.messages,
          model: request.model,
          stream: false
        });
      } catch (err) {
        this.log.error('Rust worker failed, falling back', err);
        // Fall through to legacy
      }
    }

    // Fallback to TypeScript (temporary during migration)
    return await this.legacyGenerateText(request);
  }

  async *streamText(request: TextGenerationRequest): AsyncIterableIterator<string> {
    if (this.useRustWorker && this.workerClient.isConnected()) {
      try {
        yield* this.workerClient.generateStream({
          provider: request.preferredProvider,
          messages: request.messages,
          model: request.model
        });
        return;
      } catch (err) {
        this.log.error('Rust streaming failed', err);
      }
    }

    // Fallback: buffer full response (no streaming)
    const response = await this.legacyGenerateText(request);
    yield response.text;
  }
}
```

---

## Testing Strategy

### 6-Level Testing Pyramid

```
                    ┌─────────────────────┐
                    │   Level 6: E2E      │  ← PersonaUser tasks
                    │ (Real AI agents)    │
                    └─────────────────────┘
                           ▲
                           │
                ┌──────────────────────┐
                │  Level 5: System     │  ← Through AIProviderDaemon
                │ (Full integration)   │
                └──────────────────────┘
                           ▲
                           │
            ┌──────────────────────────────┐
            │   Level 4: IPC Tests         │  ← TypeScript ↔ Rust
            │ (Client ↔ Worker)            │
            └──────────────────────────────┘
                           ▲
                           │
          ┌────────────────────────────────────┐
          │  Level 3: Standalone Worker        │  ← Full binary
          │ (Binary + Test Client)             │
          └────────────────────────────────────┘
                           ▲
                           │
        ┌────────────────────────────────────────┐
        │   Level 2: Integration Tests           │  ← Real APIs
        │ (Rust + Real Ollama/APIs)              │
        └────────────────────────────────────────┘
                           ▲
                           │
      ┌──────────────────────────────────────────────┐
      │        Level 1: Unit Tests                   │  ← Pure logic
      │  (Providers, Protocol, No Dependencies)      │
      └──────────────────────────────────────────────┘
```

### Level 1: Unit Tests (Rust)

**Goal**: Test individual modules without external dependencies

```rust
// tests/unit/ollama_tests.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_ollama_response() {
        let json = r#"{"response": "Hello", "done": true}"#;
        let response = parse_ollama_response(json).unwrap();
        assert_eq!(response.text, "Hello");
        assert!(response.done);
    }

    #[tokio::test]
    async fn test_build_ollama_request() {
        let request = GenerateRequest {
            provider: "ollama".into(),
            model: "llama3.2".into(),
            prompt: "Test".into(),
            stream: false,
        };

        let ollama_req = build_ollama_request(&request);
        assert_eq!(ollama_req.model, "llama3.2");
    }
}
```

**Run**: `cargo test --lib`

### Level 2: Integration Tests (Rust + Real Services)

**Goal**: Test with real Ollama/API services

```rust
// tests/integration/ollama_integration_test.rs
#[tokio::test]
#[ignore] // Requires Ollama running
async fn test_ollama_generate_real() {
    let request = GenerateRequest {
        provider: "ollama".into(),
        model: "llama3.2:latest".into(),
        prompt: "Say hello in 3 words".into(),
        stream: false,
    };

    let response = ollama::generate(request, &default_provider())
        .await
        .expect("Ollama should respond");

    assert!(!response.text.is_empty());
}
```

**Run**: `cargo test --test '*' -- --ignored`

### Level 3: Standalone Worker Test

**Goal**: Test full binary with simple client (no system deps)

```typescript
// tests/standalone/test-client.ts
async function testStandalone() {
  const client = new AIProviderWorkerClient('/tmp/test-ai-provider.sock');
  await client.connect();

  // Test generation
  const response = await client.generate({
    provider: 'ollama',
    model: 'llama3.2:latest',
    prompt: 'Say hello',
    stream: false
  });
  console.log('✅ Response:', response.text);

  // Test streaming
  for await (const chunk of client.generateStream({
    provider: 'ollama',
    model: 'llama3.2:latest',
    prompt: 'Count to 3'
  })) {
    process.stdout.write(chunk);
  }

  await client.disconnect();
}
```

**Run**:
```bash
# Terminal 1: Start worker
cargo run --release -- /tmp/test-ai-provider.sock

# Terminal 2: Test
npx tsx tests/standalone/test-client.ts
```

### Level 4: IPC Tests (TypeScript ↔ Rust)

**Goal**: Test TypeScript client library with worker

```typescript
// tests/integration/ai-provider-worker-ipc.test.ts
describe('AIProviderWorker IPC', () => {
  let client: AIProviderWorkerClient;

  beforeAll(async () => {
    // Worker started by test setup
    client = new AIProviderWorkerClient();
    await client.connect();
  });

  it('should generate text', async () => {
    const response = await client.generate({
      provider: 'ollama',
      model: 'llama3.2:latest',
      prompt: 'Test',
      stream: false
    });
    expect(response.text).toBeTruthy();
  });

  it('should stream tokens', async () => {
    const chunks: string[] = [];
    for await (const chunk of client.generateStream({
      provider: 'ollama',
      model: 'llama3.2:latest',
      prompt: 'Count to 3'
    })) {
      chunks.push(chunk);
    }
    expect(chunks.length).toBeGreaterThan(1);
  });
});
```

**Run**: `npx vitest tests/integration/ai-provider-worker-ipc.test.ts`

### Level 5: System Integration Tests

**Goal**: Test through AIProviderDaemon routing

```typescript
// tests/integration/ai-provider-system.test.ts
describe('AIProviderDaemon with Rust Worker', () => {
  it('should route to Rust worker', async () => {
    const result = await Commands.execute('ai/generate', {
      provider: 'ollama',
      model: 'llama3.2:latest',
      messages: [{ role: 'user', content: 'Hello' }]
    });
    expect(result.success).toBe(true);
  });

  it('should fallback on worker failure', async () => {
    // Kill worker
    exec('pkill -f ai-provider-worker');

    // Should still work via fallback
    const result = await Commands.execute('ai/generate', {
      provider: 'ollama',
      model: 'llama3.2:latest',
      messages: [{ role: 'user', content: 'Test' }]
    });
    expect(result.success).toBe(true);
  });
});
```

**Run**: `npx vitest tests/integration/ai-provider-system.test.ts`

### Level 6: E2E Tests (PersonaUser)

**Goal**: Test with real AI agents

```typescript
// tests/e2e/persona-ai-inference.test.ts
describe('PersonaUser AI Inference', () => {
  it('should complete task with streaming', async () => {
    const task = await Commands.execute('task/create', {
      assignee: 'helper-ai-id',
      description: 'Explain Rust in 2 sentences',
      priority: 0.8
    });

    await new Promise(resolve => setTimeout(resolve, 10000));

    const result = await Commands.execute('task/get', { taskId: task.id });
    expect(result.status).toBe('completed');
    expect(result.outcome).toContain('Rust');
  });
});
```

**Run**: `npx vitest tests/e2e/persona-ai-inference.test.ts --timeout=30000`

### Test-Driven Build Workflow

```bash
# 1. Unit tests (fast, no dependencies)
cargo test --lib
# ✅ Pass → Continue

# 2. Integration tests (needs Ollama)
ollama serve &
cargo test --test '*' -- --ignored
# ✅ Pass → Continue

# 3. Build release binary
cargo build --release
# ✅ Builds → Continue

# 4. Standalone test
cargo run --release -- /tmp/test.sock &
npx tsx tests/standalone/test-client.ts
# ✅ Pass → Continue

# 5. IPC tests
npx vitest tests/integration/ai-provider-worker-ipc.test.ts
# ✅ Pass → Continue

# 6. System integration
npm start
npx vitest tests/integration/ai-provider-system.test.ts
# ✅ Pass → Continue

# 7. E2E with personas
npx vitest tests/e2e/persona-ai-inference.test.ts
# ✅ Pass → Production ready!
```

---

## Implementation Plan

### Phase 1: Rust Worker (Week 1)

**Days 1-2: Project Setup + Ollama Provider**
- [ ] Create `workers/ai-provider/` structure
- [ ] Set up Cargo.toml with dependencies
- [ ] Implement `messages.rs` (JTAG protocol types)
- [ ] Implement `providers/ollama.rs` using ollama-rs
- [ ] Write unit tests for Ollama provider
- [ ] Write integration test with real Ollama
- [ ] **Milestone**: Ollama provider works standalone

**Days 3-4: External API Providers**
- [ ] Implement `providers/anthropic.rs` (Claude)
- [ ] Implement `providers/openai.rs` (GPT)
- [ ] Implement `providers/grok.rs`
- [ ] Add streaming support for all providers
- [ ] Write unit tests for each provider
- [ ] **Milestone**: All providers implemented

**Days 5-7: Worker Core + Health**
- [ ] Implement `main.rs` (socket listener)
- [ ] Implement `connection_handler.rs` (message routing)
- [ ] Implement `health.rs` (monitoring)
- [ ] Implement embeddings support
- [ ] Add LoggerWorker integration
- [ ] Write standalone test client
- [ ] **Milestone**: Full worker binary working

### Phase 2: TypeScript Client + Integration (Week 2)

**Days 1-2: TypeScript Client**
- [ ] Create `shared/ipc/ai-provider/AIProviderWorkerClient.ts`
- [ ] Implement `generate()` method
- [ ] Implement `generateStream()` AsyncIterator
- [ ] Implement `embeddings()` method
- [ ] Implement `ping()` health check
- [ ] Write IPC tests
- [ ] **Milestone**: TypeScript client works with worker

**Days 3-5: AIProviderDaemon Integration**
- [ ] Add `AIProviderWorkerClient` to AIProviderDaemon
- [ ] Add feature flag: `USE_RUST_AI_PROVIDER`
- [ ] Implement fallback logic (Rust → TypeScript)
- [ ] Add shadow testing (compare results)
- [ ] Deploy with flag OFF
- [ ] **Milestone**: Worker runs but not used

**Days 6-7: Shadow Testing**
- [ ] Enable shadow mode (send to both, use TS result)
- [ ] Log differences between Rust and TS results
- [ ] Fix any discrepancies
- [ ] Monitor performance metrics
- [ ] **Milestone**: Rust matches TypeScript behavior

### Phase 3: Gradual Rollout (Week 3)

**Days 1-2: Ollama Cutover**
- [ ] Enable Rust for Ollama only
- [ ] Monitor logs for errors
- [ ] Test with PersonaUser
- [ ] Verify streaming works
- [ ] **Milestone**: Ollama via Rust in production

**Days 3-4: External APIs Cutover**
- [ ] Enable Anthropic via Rust
- [ ] Enable OpenAI via Rust
- [ ] Enable Grok via Rust
- [ ] Monitor latency improvements
- [ ] **Milestone**: All providers via Rust

**Days 5-7: Full Migration**
- [ ] Remove TypeScript fallback code
- [ ] Update documentation
- [ ] Run full E2E test suite
- [ ] Performance benchmarking
- [ ] **Milestone**: Production migration complete

### Phase 4: Optimization (Week 4)

**Days 1-3: Performance Tuning**
- [ ] Connection pooling for HTTP clients
- [ ] Request batching where applicable
- [ ] Streaming chunk size optimization
- [ ] Memory usage profiling
- [ ] **Milestone**: Performance optimized

**Days 4-7: LoRA Preparation**
- [ ] Add LoRA adapter loading infrastructure
- [ ] Implement model management APIs
- [ ] Test with dummy LoRA adapter
- [ ] Document LoRA integration points
- [ ] **Milestone**: Ready for LoRA phase

---

## Module Specifications

### main.rs

```rust
use std::os::unix::net::UnixListener;
use tokio::task;

mod connection_handler;
mod providers;
mod health;
mod messages;

#[tokio::main]
async fn main() -> Result<()> {
    let socket_path = std::env::args()
        .nth(1)
        .expect("Usage: ai-provider-worker <socket-path>");

    let listener = UnixListener::bind(&socket_path)?;
    println!("🤖 AIProviderWorker listening on {}", socket_path);

    for stream in listener.incoming() {
        let stream = stream?;
        task::spawn(async move {
            connection_handler::handle_client(stream).await
        });
    }

    Ok(())
}
```

### connection_handler.rs

```rust
use tokio::net::UnixStream;
use crate::messages::*;
use crate::providers;

pub async fn handle_client(mut stream: UnixStream) -> Result<()> {
    loop {
        let request: JTAGRequest<ProviderRequest> = read_message(&mut stream).await?;

        let response = match request.payload.action.as_str() {
            "generate" => providers::generate(request.payload).await,
            "generate-stream" => providers::generate_stream(&mut stream, request).await,
            "embeddings" => providers::embeddings(request.payload).await,
            "ping" => health::ping().await,
            _ => Err(Error::UnknownAction)
        };

        send_response(&mut stream, response).await?;
    }
}
```

### providers/ollama.rs

```rust
use ollama_rs::{Ollama, generation::completion::GenerationRequest};
use tokio_stream::StreamExt;

pub async fn generate(request: GenerateRequest) -> Result<GenerateResponse> {
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
    request: GenerateRequest
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

### providers/anthropic.rs

```rust
use reqwest::Client;

pub async fn generate(request: GenerateRequest) -> Result<GenerateResponse> {
    let client = Client::new();

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &request.api_key)
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

---

## Deployment Guide

### Development
```bash
# Build and start worker
npm run worker:start

# Worker auto-discovered by modular launcher
# Socket: /tmp/jtag-ai-provider-worker.sock
```

### Testing
```bash
# All tests
npm run test:ai-worker

# Specific level
npm run test:ai-worker:unit
npm run test:ai-worker:integration
npm run test:ai-worker:ipc
npm run test:ai-worker:system
npm run test:ai-worker:e2e
```

### Feature Flag Control
```bash
# Enable Rust worker
export USE_RUST_AI_PROVIDER=true
npm start

# Disable (use TypeScript fallback)
export USE_RUST_AI_PROVIDER=false
npm start

# Per-provider control (future)
export USE_RUST_OLLAMA=true
export USE_RUST_ANTHROPIC=false
```

### Monitoring
```bash
# Worker status
npm run worker:status

# Health check
./jtag ai/ping

# Worker logs
tail -f .continuum/jtag/logs/system/rust-workers/ai-provider.log
```

### Rollback
```bash
# Instant rollback to TypeScript
export USE_RUST_AI_PROVIDER=false
npm run restart:quick

# Or kill worker (auto-fallback)
pkill -f ai-provider-worker
```

---

## Success Criteria

✅ All unit tests pass (90%+ coverage)
✅ All integration tests pass (real APIs work)
✅ IPC tests pass (TypeScript ↔ Rust communication)
✅ System tests pass (routing + fallback)
✅ E2E tests pass (PersonaUser completes tasks)
✅ Performance: 2-5x latency reduction
✅ Streaming works (real-time tokens)
✅ Zero downtime during migration
✅ Feature flag allows instant rollback
✅ All providers working via Rust

---

## Next Steps

**Ready to build!** Start with Phase 1, Day 1:
1. Create `workers/ai-provider/` structure
2. Set up Cargo.toml
3. Implement `messages.rs`
4. Build Ollama provider with tests

**Testing at every step ensures quality and confidence.**

---

**Document Version**: 1.0
**Last Updated**: 2025-12-11
**Status**: ✅ Ready for Implementation
