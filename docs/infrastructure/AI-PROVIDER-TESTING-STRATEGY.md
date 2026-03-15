**LEGACY**: This document references Ollama which is no longer used. Local inference is now Candle-based (Rust, in-process). This doc is kept for historical reference only.

# AIProviderWorker Testing Strategy - Test Every Level

## Architecture Pattern: Daemon Owns Connection

**Pattern Reference**: LoggerDaemon (working implementation)

This worker uses the **daemon-owns-connection** pattern:
- AIProviderDaemon (TypeScript) owns AIProviderWorkerClient (direct socket connection)
- No child process spawning (worker started independently by start-workers.sh)
- TypeScript defines protocol (source of truth → codegen → Rust)
- Rust implements efficiently (systems level)

**Key Testing Implication**: Tests focus on socket communication, not process lifecycle management.

## Philosophy: Test in Isolation, Then Integrate

Each component is testable independently BEFORE integrating with the next layer.

## Testing Pyramid

```
                    ┌─────────────────────┐
                    │   System Tests      │  ← PersonaUser end-to-end
                    │ (Full integration)  │
                    └─────────────────────┘
                           ▲
                           │
                ┌──────────────────────┐
                │  IPC Tests           │  ← TypeScript ↔ Rust
                │ (Client ↔ Worker)    │
                └──────────────────────┘
                           ▲
                           │
            ┌──────────────────────────────┐
            │   Integration Tests          │  ← Rust worker with real APIs
            │ (Worker + Real Ollama/APIs)  │
            └──────────────────────────────┘
                           ▲
                           │
          ┌────────────────────────────────────┐
          │        Unit Tests                  │  ← Individual modules
          │  (Providers, Protocol, Parsers)    │
          └────────────────────────────────────┘
```

## Level 1: Unit Tests (Rust - No External Dependencies)

Test individual provider modules with mocked HTTP/Ollama responses.

### Test Files Structure
```
workers/ai-provider/
├── src/
│   ├── providers/
│   │   ├── ollama.rs
│   │   ├── anthropic.rs
│   │   └── openai.rs
│   └── messages.rs
└── tests/
    ├── unit/
    │   ├── ollama_tests.rs
    │   ├── anthropic_tests.rs
    │   ├── openai_tests.rs
    │   └── messages_tests.rs
    └── ...
```

### Example: Ollama Provider Unit Test
```rust
// tests/unit/ollama_tests.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_ollama_response() {
        let json = r#"{
            "response": "Hello world",
            "done": true,
            "total_duration": 5000000
        }"#;

        let response = parse_ollama_response(json).unwrap();
        assert_eq!(response.text, "Hello world");
        assert_eq!(response.done, true);
    }

    #[tokio::test]
    async fn test_build_ollama_request() {
        let request = GenerateRequest {
            provider: "ollama".into(),
            model: "llama3.2".into(),
            prompt: "Test prompt".into(),
            stream: false,
        };

        let ollama_req = build_ollama_request(&request);
        assert_eq!(ollama_req.model, "llama3.2");
        assert_eq!(ollama_req.prompt, "Test prompt");
        assert_eq!(ollama_req.stream, false);
    }
}
```

### Run Unit Tests
```bash
cd workers/ai-provider
cargo test --lib

# Test specific module
cargo test --lib ollama_tests

# With output
cargo test --lib -- --nocapture
```

**Status**: ✅ Passes = Module logic correct, ready for integration

---

## Level 2: Integration Tests (Rust - Real APIs)

Test worker with real Ollama/API services running.

### Test Files Structure
```
workers/ai-provider/tests/
└── integration/
    ├── ollama_integration_test.rs
    ├── anthropic_integration_test.rs
    └── streaming_test.rs
```

### Example: Ollama Integration Test
```rust
// tests/integration/ollama_integration_test.rs
use ai_provider_worker::providers::ollama;

#[tokio::test]
#[ignore] // Requires Ollama running
async fn test_ollama_generate_real() {
    // This hits real Ollama service at localhost:11434
    let request = GenerateRequest {
        provider: "ollama".into(),
        model: "llama3.2:latest".into(),
        prompt: "Say hello in 3 words".into(),
        stream: false,
    };

    let response = ollama::generate(request, &default_provider())
        .await
        .expect("Ollama should generate response");

    assert!(!response.text.is_empty());
    assert!(response.text.len() < 100); // Should be short
    println!("Response: {}", response.text);
}

#[tokio::test]
#[ignore]
async fn test_ollama_streaming_real() {
    let request = GenerateRequest {
        provider: "ollama".into(),
        model: "llama3.2:latest".into(),
        prompt: "Count to 5".into(),
        stream: true,
    };

    let mut stream = ollama::generate_stream(request, &default_provider())
        .await
        .expect("Should create stream");

    let mut chunks = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.expect("Chunk should be valid");
        chunks.push(chunk);
    }

    assert!(chunks.len() > 1, "Should receive multiple chunks");
    println!("Received {} chunks", chunks.len());
}
```

### Run Integration Tests
```bash
# Ensure Ollama is running first
ollama serve &

# Run integration tests
cd workers/ai-provider
cargo test --test '*' -- --ignored

# Test specific integration
cargo test --test ollama_integration_test -- --ignored --nocapture
```

**Status**: ✅ Passes = Worker correctly talks to real APIs

---

## Level 3: Worker Standalone Tests (No System Integration)

Test the full worker binary with a simple TypeScript client (no system deps).

### Standalone Test Client
```typescript
// workers/ai-provider/test/standalone-client.ts
import { AIProviderWorkerClient } from './AIProviderWorkerClient';

async function testStandalone() {
  console.log('🧪 Testing AIProviderWorker standalone...');

  const client = new AIProviderWorkerClient('/tmp/test-ai-provider.sock');
  await client.connect();

  // Test 1: Ollama generation
  console.log('\n📝 Test 1: Ollama generation');
  const response = await client.generate({
    provider: 'ollama',
    model: 'llama3.2:latest',
    prompt: 'Say hello',
    stream: false
  });
  console.log('✅ Response:', response.text);

  // Test 2: Streaming
  console.log('\n🌊 Test 2: Streaming generation');
  const chunks: string[] = [];
  for await (const chunk of client.generateStream({
    provider: 'ollama',
    model: 'llama3.2:latest',
    prompt: 'Count to 3'
  })) {
    chunks.push(chunk);
    process.stdout.write(chunk);
  }
  console.log(`\n✅ Received ${chunks.length} chunks`);

  // Test 3: Embeddings
  console.log('\n🔢 Test 3: Embeddings');
  const embeddings = await client.embeddings({
    provider: 'ollama',
    model: 'nomic-embed-text',
    text: 'This is a test'
  });
  console.log(`✅ Embedding dimension: ${embeddings.length}`);

  // Test 4: Health check
  console.log('\n❤️  Test 4: Health check');
  const health = await client.ping();
  console.log('✅ Health:', health);

  await client.disconnect();
  console.log('\n✅ All standalone tests passed!');
}

testStandalone().catch(console.error);
```

### Run Standalone Tests
```bash
# Terminal 1: Start worker manually
cd workers/ai-provider
cargo run --release -- /tmp/test-ai-provider.sock

# Terminal 2: Run test client
npx tsx workers/ai-provider/test/standalone-client.ts
```

**Status**: ✅ Passes = Worker IPC protocol works correctly

---

## Level 4: IPC Tests (TypeScript Client ↔ Rust Worker)

Test TypeScript client library with real worker.

### IPC Test Suite
```typescript
// tests/integration/ai-provider-worker-ipc.test.ts
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { AIProviderWorkerClient } from '@shared/ipc/ai-provider/AIProviderWorkerClient';
import { spawn, ChildProcess } from 'child_process';

describe('AIProviderWorker IPC', () => {
  let worker: ChildProcess;
  let client: AIProviderWorkerClient;
  const socketPath = '/tmp/test-ai-provider-ipc.sock';

  beforeAll(async () => {
    // Start worker
    worker = spawn(
      'workers/ai-provider/target/release/ai-provider-worker',
      [socketPath],
      { stdio: 'ignore', detached: true }
    );
    worker.unref();

    // Wait for socket
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Connect client
    client = new AIProviderWorkerClient(socketPath);
    await client.connect();
  });

  afterAll(async () => {
    await client.disconnect();
    if (worker.pid) process.kill(worker.pid);
  });

  it('should generate text via Ollama', async () => {
    const response = await client.generate({
      provider: 'ollama',
      model: 'llama3.2:latest',
      prompt: 'Say hello',
      stream: false
    });

    expect(response.text).toBeTruthy();
    expect(response.text.length).toBeGreaterThan(0);
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
    expect(chunks.join('')).toBeTruthy();
  });

  it('should generate embeddings', async () => {
    const embeddings = await client.embeddings({
      provider: 'ollama',
      model: 'nomic-embed-text',
      text: 'Test text'
    });

    expect(Array.isArray(embeddings)).toBe(true);
    expect(embeddings.length).toBeGreaterThan(0);
  });

  it('should handle errors gracefully', async () => {
    await expect(
      client.generate({
        provider: 'nonexistent',
        model: 'fake',
        prompt: 'Test',
        stream: false
      })
    ).rejects.toThrow();
  });

  it('should report health status', async () => {
    const health = await client.ping();

    expect(health.providers).toBeDefined();
    expect(health.uptime_ms).toBeGreaterThan(0);
  });
});
```

### Run IPC Tests
```bash
# Build worker first
npm run worker:build

# Run IPC test suite
npx vitest tests/integration/ai-provider-worker-ipc.test.ts
```

**Status**: ✅ Passes = Client-worker communication works

---

## Level 5: System Integration Tests (With AIProviderDaemon)

Test through the full system with AIProviderDaemon routing.

**Key Pattern Test**: Daemon owns WorkerClient connection (LoggerDaemon pattern)
- Daemon connects to worker socket (no ProcessManager spawning)
- Daemon provides lifecycle management (start/stop/health)
- Daemon handles fallback to TypeScript adapters
- Worker runs independently (started by start-workers.sh)

### System Integration Test
```typescript
// tests/integration/ai-provider-system.test.ts
import { describe, it, expect, beforeAll } from 'vitest';
import { Commands } from '@system/core/shared/Commands';

describe('AIProviderDaemon with Rust Worker', () => {
  beforeAll(async () => {
    // Ensure system is running with worker enabled
    process.env.USE_RUST_AI_PROVIDER = 'true';
  });

  it('should route generation to Rust worker', async () => {
    const result = await Commands.execute('ai/generate', {
      provider: 'ollama',
      model: 'llama3.2:latest',
      messages: [
        { role: 'user', content: 'Say hello in 3 words' }
      ]
    });

    expect(result.success).toBe(true);
    expect(result.text).toBeTruthy();
  });

  it('should test daemon owns connection pattern', async () => {
    // Verify daemon is connected to worker (not spawning child process)
    const health = await Commands.execute('ai/ping', {});

    expect(health.success).toBe(true);
    expect(health.connectionType).toBe('socket'); // Not 'child-process'
  });

  it('should fallback to TypeScript on worker failure', async () => {
    // Simulate worker failure by killing it
    const { exec } = require('child_process');
    exec('pkill -f ai-provider-worker');

    await new Promise(resolve => setTimeout(resolve, 1000));

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

### Run System Tests
```bash
# Ensure system is running
npm start

# Run system integration tests
npx vitest tests/integration/ai-provider-system.test.ts
```

**Status**: ✅ Passes = Full system integration works

---

## Level 6: End-to-End Tests (PersonaUser)

Test with real PersonaUser autonomous loop.

### E2E Test
```typescript
// tests/e2e/persona-ai-inference.test.ts
import { describe, it, expect } from 'vitest';
import { Commands } from '@system/core/shared/Commands';

describe('PersonaUser AI Inference (E2E)', () => {
  it('should complete autonomous task with streaming', async () => {
    // Create test task for Helper AI
    const task = await Commands.execute('task/create', {
      assignee: 'helper-ai-id',
      description: 'Explain what Rust is in 2 sentences',
      priority: 0.8
    });

    // Wait for persona to process
    await new Promise(resolve => setTimeout(resolve, 10000));

    // Verify task completed
    const taskResult = await Commands.execute('task/get', {
      taskId: task.id
    });

    expect(taskResult.status).toBe('completed');
    expect(taskResult.outcome).toContain('Rust');
  });
});
```

### Run E2E Tests
```bash
npm start
npx vitest tests/e2e/persona-ai-inference.test.ts --timeout=30000
```

**Status**: ✅ Passes = Real AI agents working with Rust worker

---

## Testing Workflow (Build Order)

```bash
# Step 1: Unit tests (no dependencies)
cd workers/ai-provider
cargo test --lib
# ✅ Pass → Continue

# Step 2: Integration tests (needs Ollama running)
ollama serve &
cargo test --test '*' -- --ignored
# ✅ Pass → Continue

# Step 3: Build release binary
cargo build --release
# ✅ Builds → Continue

# Step 4: Standalone client test
cargo run --release -- /tmp/test.sock &
npx tsx workers/ai-provider/test/standalone-client.ts
# ✅ Pass → Continue

# Step 5: IPC tests (TypeScript ↔ Rust)
npx vitest tests/integration/ai-provider-worker-ipc.test.ts
# ✅ Pass → Continue

# Step 6: Deploy to system (flag OFF)
npm run build
npm start
# ✅ Worker runs but not used → Continue

# Step 7: System integration tests (with fallback)
npx vitest tests/integration/ai-provider-system.test.ts
# ✅ Pass → Continue

# Step 8: Enable flag and test
USE_RUST_AI_PROVIDER=true npm start
# ✅ System works → Continue

# Step 9: E2E tests with real personas
npx vitest tests/e2e/persona-ai-inference.test.ts
# ✅ Pass → Production ready!
```

## Test Coverage Goals

- **Unit tests**: 90%+ coverage of provider modules
- **Integration tests**: All providers + streaming + embeddings
- **IPC tests**: All message types + error handling
- **System tests**: Routing + fallback + health monitoring
- **E2E tests**: Real PersonaUser tasks complete successfully

## Continuous Testing

```typescript
// .github/workflows/ai-provider-worker-tests.yml
name: AI Provider Worker Tests

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Rust unit tests
        run: |
          cd workers/ai-provider
          cargo test --lib

  integration-tests:
    runs-on: ubuntu-latest
    services:
      ollama:
        image: ollama/ollama:latest
    steps:
      - uses: actions/checkout@v2
      - name: Pull test model
        run: ollama pull llama3.2:latest
      - name: Rust integration tests
        run: |
          cd workers/ai-provider
          cargo test --test '*' -- --ignored

  ipc-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Build worker
        run: |
          cd workers/ai-provider
          cargo build --release
      - name: TypeScript IPC tests
        run: npx vitest tests/integration/ai-provider-worker-ipc.test.ts
```

## Quick Test Commands

```bash
# Test everything at once (requires Ollama running)
npm run test:ai-worker

# Test just unit tests (fast, no dependencies)
npm run test:ai-worker:unit

# Test with real APIs (slow, needs services)
npm run test:ai-worker:integration

# Test IPC only
npm run test:ai-worker:ipc

# Test system integration
npm run test:ai-worker:system

# Test E2E with personas
npm run test:ai-worker:e2e
```

## Summary

✅ **Every component is testable in isolation**
✅ **Each level builds on previous level's passing tests**
✅ **No integration until unit tests pass**
✅ **No system deployment until IPC tests pass**
✅ **No production until E2E tests pass**

**Result: Confidence at every step, no surprises in production!**
