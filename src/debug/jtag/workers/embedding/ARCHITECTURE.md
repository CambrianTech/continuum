# Rust Embedding Worker Architecture

## Why Native Rust Embeddings?

**Current pain point**: Embedding generation goes through Ollama HTTP API:
- HTTP serialization overhead per request
- JSON encoding/decoding of 384-dim float arrays
- Depends on external Ollama service being healthy
- Single-threaded request queue (artificial maxConcurrent)
- ~80ms per embedding via HTTP, but should be ~5ms native

**Solution**: Generate embeddings directly in Rust using `fastembed-rs`:
- No network overhead
- Batch multiple texts in single call
- True parallelism via rayon
- Model loaded once, reused for all requests

## fastembed-rs Overview

Based on [fastembed crate](https://crates.io/crates/fastembed):

```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

// Load model (auto-downloads from HuggingFace on first use)
let model = TextEmbedding::try_new(
    InitOptions::new(EmbeddingModel::AllMiniLML6V2)
        .with_cache_dir(".continuum/models")
        .with_show_download_progress(true)
)?;

// Batch embed - parallelized internally
let embeddings = model.embed(vec![
    "memory content 1",
    "memory content 2",
    "memory content 3",
], None)?;  // None = default batch size (256)
```

Key features:
- Uses ONNX Runtime via `ort` crate (fast, production-ready)
- Auto-downloads models from HuggingFace
- Supports quantized models (smaller, faster)
- No Tokio dependency (sync API)

## Supported Models

| Model | Dimensions | Size | Use Case |
|-------|-----------|------|----------|
| AllMiniLML6V2 | 384 | ~90MB | Fast, good quality |
| AllMiniLML6V2Q | 384 | ~25MB | Quantized, fastest |
| BGESmallENV15 | 384 | ~130MB | Better quality |
| BGEBaseENV15 | 768 | ~440MB | Best quality |
| NomicEmbedTextV15 | 768 | ~550MB | Nomic (same as Ollama) |

Default: **AllMiniLML6V2** - matches current Ollama embedding dimensions (384).

## Architecture Decision: Dedicated Worker vs Extension

**Option A: Extend data-daemon-worker**
- Pro: Single worker, embeddings close to data
- Pro: Can auto-embed on data/create
- Con: Model loading adds memory to data worker
- Con: Mixing concerns (data ops vs ML inference)

**Option B: Dedicated embedding-worker** (CHOSEN)
- Pro: Isolation - embedding crashes don't affect data
- Pro: Can scale independently
- Pro: Clean separation of concerns
- Pro: Matches existing worker pattern (data, archive, search, logger)
- Con: One more worker to manage

## Request/Response Protocol

Same Unix socket + newline-delimited JSON pattern as other workers:

```rust
#[derive(Deserialize)]
#[serde(tag = "command")]
enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "embedding/generate")]
    Generate {
        texts: Vec<String>,
        model: Option<String>,  // default: AllMiniLML6V2
    },

    #[serde(rename = "embedding/model/load")]
    ModelLoad {
        model: String,
    },

    #[serde(rename = "embedding/model/list")]
    ModelList,

    #[serde(rename = "embedding/model/info")]
    ModelInfo {
        model: String,
    },
}

#[derive(Serialize)]
#[serde(tag = "status")]
enum Response {
    #[serde(rename = "ok")]
    Ok { data: Value },

    #[serde(rename = "error")]
    Error { message: String },
}
```

### Generate Response Format

```json
{
  "status": "ok",
  "data": {
    "embeddings": [[0.1, 0.2, ...], [0.3, 0.4, ...]],
    "model": "AllMiniLML6V2",
    "dimensions": 384,
    "count": 2,
    "durationMs": 12
  }
}
```

## TypeScript Integration

New `RustEmbeddingClient` in `system/core/services/`:

```typescript
export class RustEmbeddingClient {
  private socketPath: string = '/tmp/jtag-embedding.sock';

  async generate(texts: string[]): Promise<number[][]> {
    const response = await this.send({
      command: 'embedding/generate',
      texts
    });
    return response.data.embeddings;
  }

  async isAvailable(): Promise<boolean> {
    try {
      await this.send({ command: 'ping' });
      return true;
    } catch {
      return false;
    }
  }
}
```

Update `EmbeddingService` to use Rust when available:

```typescript
class EmbeddingService {
  private static rustClient = new RustEmbeddingClient();

  static async generateEmbedding(text: string): Promise<number[]> {
    // Try Rust first (fast, local)
    if (await this.rustClient.isAvailable()) {
      const [embedding] = await this.rustClient.generate([text]);
      return embedding;
    }

    // Fallback to Ollama HTTP
    return this.generateViaOllama(text);
  }
}
```

## Model Caching Strategy

```
~/.continuum/
├── models/
│   └── fastembed/
│       ├── AllMiniLML6V2/
│       │   ├── model.onnx
│       │   ├── tokenizer.json
│       │   └── config.json
│       └── BGESmallENV15/
│           └── ...
```

- Models auto-download on first use
- Cache persists across restarts
- Support `FASTEMBED_CACHE_PATH` env var override

## Performance Expectations

| Metric | Ollama HTTP | Rust Native |
|--------|-------------|-------------|
| Single text | ~80ms | ~5ms |
| Batch 10 | ~800ms | ~15ms |
| Batch 100 | ~8s | ~100ms |
| Memory | External | ~200MB (model) |

## Implementation Plan

1. **Create Cargo project** (`workers/embedding/`)
2. **Implement core**: Ping, Generate, ModelLoad
3. **Add TypeScript client**: `RustEmbeddingClient.ts`
4. **Update EmbeddingService**: Use Rust when available
5. **Add to workers-config.json**: Enable by default
6. **Update start/stop scripts**: Include embedding worker

## Cargo.toml

```toml
[package]
name = "embedding-worker"
version = "0.1.0"
edition = "2021"

[dependencies]
fastembed = "4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

Note: `fastembed` pulls in `ort` (ONNX Runtime) which has native binaries.
First build will download ONNX runtime (~200MB).
