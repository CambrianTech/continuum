# Rust Worker Registration Pattern

## The Rust-First Principle

**CRITICAL ARCHITECTURAL RULE:**

> **Complex algorithmic work = Rust. Always. Never Node.**

If you're implementing an algorithm, computation, or CPU-intensive operation, it belongs in a Rust worker. Node.js is ONLY for orchestration (routing, WebSockets, simple coordination).

**Examples of what belongs in Rust:**
- Search algorithms (BM25, vector search)
- ML inference (Whisper, Piper, embeddings)
- Audio/video processing (VAD, mixing)
- Image processing (filters, detection)
- Database operations (SQLite, indexing)
- File I/O (large files, streaming)
- Any loop over large datasets

**Node.js is for:**
- HTTP routing
- WebSocket connection management
- Event bus coordination
- Calling Rust workers

---

## Problem

Every time we add a new Rust adapter/module, registration is error-prone. Need a formulaic, checklist-driven approach.

## The Pattern: Polymorphic Registry System

Based on OpenCV's `cv::Algorithm` pattern - used successfully in `system/genome/` adapters.

### Universal Registration Pattern (5 Steps)

#### Step 1: Define the Trait (Interface)

```rust
//! mod.rs - Define the trait ALL implementations must follow

#[async_trait]
pub trait YourAdapter: Send + Sync {
    /// Adapter name (e.g., "piper", "kokoro")
    fn name(&self) -> &'static str;

    /// Human-readable description
    fn description(&self) -> &'static str;

    /// Check if adapter is ready
    fn is_initialized(&self) -> bool;

    /// Initialize (load models, connect APIs, etc.)
    async fn initialize(&self) -> Result<(), YourError>;

    /// Core functionality
    async fn execute(&self, input: &Input) -> Result<Output, YourError>;
}
```

#### Step 2: Create the Registry

```rust
//! mod.rs - Registry manages all implementations

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use once_cell::sync::OnceCell;

static REGISTRY: OnceCell<Arc<RwLock<AdapterRegistry>>> = OnceCell::new();

pub struct AdapterRegistry {
    adapters: HashMap<&'static str, Arc<dyn YourAdapter>>,
    active: Option<&'static str>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
            active: None,
        }
    }

    pub fn register(&mut self, adapter: Arc<dyn YourAdapter>) {
        let name = adapter.name();
        tracing::info!("Registry: Registering adapter '{}'", name);
        self.adapters.insert(name, adapter);

        // Auto-select first adapter as active
        if self.active.is_none() {
            self.active = Some(name);
        }
    }

    pub fn get_active(&self) -> Option<Arc<dyn YourAdapter>> {
        self.active
            .and_then(|name| self.adapters.get(name))
            .cloned()
    }
}
```

#### Step 3: Initialize Registry (ONE TIME)

```rust
//! mod.rs - init_registry() registers ALL implementations

pub fn init_registry() {
    let registry = REGISTRY.get_or_init(|| {
        let mut reg = AdapterRegistry::new();

        // ✅ Register ALL implementations here
        reg.register(Arc::new(PrimaryAdapter::new()));
        reg.register(Arc::new(AlternativeAdapter::new()));
        reg.register(Arc::new(FallbackAdapter::new()));

        Arc::new(RwLock::new(reg))
    });

    tracing::info!(
        "Registry initialized with {} adapters",
        registry.read().adapters.len()
    );
}
```

#### Step 4: Add New Implementation (3 Sub-Steps)

##### 4a. Create the Adapter File

```rust
//! new_adapter.rs - Implement the trait

use super::{YourAdapter, YourError, Input, Output};
use async_trait::async_trait;

pub struct NewAdapter {
    // ... fields
}

impl NewAdapter {
    pub fn new() -> Self {
        Self { /* ... */ }
    }
}

#[async_trait]
impl YourAdapter for NewAdapter {
    fn name(&self) -> &'static str {
        "new-adapter"  // ✅ Unique name
    }

    fn description(&self) -> &'static str {
        "Description of what this does"
    }

    // ... implement all required methods
}
```

##### 4b. Export in mod.rs

```rust
//! mod.rs - Add module and pub use

mod primary_adapter;
mod alternative_adapter;
mod new_adapter;  // ✅ ADD THIS

pub use primary_adapter::PrimaryAdapter;
pub use alternative_adapter::AlternativeAdapter;
pub use new_adapter::NewAdapter;  // ✅ ADD THIS
```

##### 4c. Register in init_registry()

```rust
pub fn init_registry() {
    let registry = REGISTRY.get_or_init(|| {
        let mut reg = AdapterRegistry::new();

        reg.register(Arc::new(PrimaryAdapter::new()));
        reg.register(Arc::new(AlternativeAdapter::new()));
        reg.register(Arc::new(NewAdapter::new()));  // ✅ ADD THIS

        Arc::new(RwLock::new(reg))
    });
}
```

#### Step 5: Verify Registration

```bash
# In tests or logs, check:
# "Registry initialized with N adapters"  # Should increment by 1
# "Registry: Registering adapter 'new-adapter'"  # Should see your name
```

---

## Checklist for Adding New Adapter

**Use this every time:**

- [ ] **Create adapter file** (`my_adapter.rs`)
- [ ] **Implement trait** (all required methods)
- [ ] **Add `mod my_adapter;`** to `mod.rs`
- [ ] **Add `pub use my_adapter::MyAdapter;`** to `mod.rs`
- [ ] **Add `reg.register(Arc::new(MyAdapter::new()));`** to `init_registry()`
- [ ] **Rebuild**: `cargo build --release`
- [ ] **Restart worker**: Check logs for "Registering adapter 'my-adapter'"
- [ ] **Verify count**: Registry count should increase by 1

---

## Anti-Pattern Detection

**Search for violations:**

```bash
# Find hard-coded adapter lists (BAD)
grep -r "match.*name.*{" workers/ --include="*.rs" | grep -v "// OK"

# Find switch statements on adapter names (BAD)
grep -r "match.*adapter.*{" workers/ --include="*.rs"

# Should find ZERO matches outside of test code
```

**If you find matches**, refactor to use dynamic registry lookup instead:

```rust
// ❌ BAD - Hard-coded list
match adapter_name {
    "piper" => PiperAdapter::new(),
    "kokoro" => KokoroAdapter::new(),
    _ => panic!("Unknown adapter"),
}

// ✅ GOOD - Dynamic lookup
let adapter = registry.get(adapter_name)
    .ok_or(Error::AdapterNotFound(adapter_name))?;
```

---

## Real-World Examples in Codebase

### Working Examples:
- ✅ **STT adapters**: `workers/streaming-core/src/stt/mod.rs`
  - Whisper, Stub adapters
  - Uses registry pattern

- ✅ **Fine-tuning adapters**: `system/genome/fine-tuning/server/adapters/`
  - PEFT, AWS, Fireworks, Ollama adapters
  - Excellent example of polymorphic registry

### Broken Example (Fixed):
- ⚠️ **TTS adapters**: `workers/streaming-core/src/tts/mod.rs`
  - Was missing Piper registration in `init_registry()`
  - **Fixed**: Added `reg.register(Arc::new(PiperTTS::new()));`

---

## Why This Works

1. **Single source of truth**: `init_registry()` is the ONLY place that knows all adapters
2. **Compile-time safety**: Trait ensures all adapters have same interface
3. **Runtime flexibility**: Can swap adapters without recompilation
4. **Discoverable**: Log shows all registered adapters at startup
5. **No duplication**: Adding adapter touches exactly 3 locations (file, mod.rs export, init_registry)

---

## Generator Opportunity

**Future**: Create generator for Rust adapters

```bash
npx tsx generator/generate-rust-adapter.ts \
  --type=tts \
  --name=MyTTS \
  --description="My TTS implementation"
```

Would generate:
- `my_tts.rs` with trait stub
- Add to `mod.rs` exports
- Add to `init_registry()` with TODO marker
- Update tests

**Why this matters**: Eliminates manual errors in registration pattern.

---

## Summary

**The Rule**: Every new Rust adapter follows this 5-step pattern. No exceptions. Use the checklist. Verify logs show registration.

If you skip a step, the adapter won't be discovered at runtime. The checklist prevents this.
