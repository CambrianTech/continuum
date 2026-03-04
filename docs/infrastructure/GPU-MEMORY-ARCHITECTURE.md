**Parent:** [Infrastructure](README.md)

# GPU Memory Architecture

**Status**: COMPLETE (Phase A+B wired)
**Date**: 2026-03-02

## Overview

The GPU Memory Manager provides unified VRAM coordination across all GPU consumers in Continuum. It detects real GPU hardware via Metal (macOS) or CUDA (Linux/Windows), allocates per-subsystem budgets, tracks allocations via RAII guards, and reports pressure levels for eviction decisions.

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                  GpuMemoryManager                    │
│                  (Arc<>, singleton)                   │
│                                                      │
│  ┌──────────────────────────────────────────────┐   │
│  │  Hardware Detection (Metal / CUDA / None)     │   │
│  │  total_vram_bytes: u64                        │   │
│  │  gpu_name: String                             │   │
│  └──────────────────────────────────────────────┘   │
│                                                      │
│  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐   │
│  │  Inference   │ │     TTS     │ │  Rendering   │   │
│  │  75% budget  │ │  10% budget │ │  10% budget  │   │
│  │  AtomicU64   │ │  AtomicU64  │ │  AtomicU64   │   │
│  └─────────────┘ └─────────────┘ └─────────────┘   │
│                                                      │
│  Reserve: 5% (headroom to prevent OOM)               │
│  Pressure watch channel (tokio::sync::watch)         │
└─────────────────────────────────────────────────────┘
         ▲                ▲                ▲
         │                │                │
    ┌────┴────┐     ┌────┴────┐     ┌────┴────┐
    │ Candle  │     │ Kokoro  │     │  Bevy   │
    │Adapter  │     │  TTS    │     │Renderer │
    │         │     │         │     │         │
    │ model_  │     │ KOKORO_ │     │ GpuGua- │
    │ guard   │     │ GPU_    │     │ rds     │
    │ adapter_│     │ GUARD   │     │ (Res.)  │
    │ guards  │     │ (Once   │     │         │
    │         │     │  Lock)  │     │ render_ │
    │ Genome  │     │         │     │ targets │
    │ Paging  │     └─────────┘     │ model_  │
    │ Engine  │                     │ guards  │
    │ alloc_  │                     └─────────┘
    │ guards  │
    └─────────┘
```

## Subsystem Budgets

| Subsystem | Budget | Consumers | Guard Location |
|-----------|--------|-----------|----------------|
| **Inference** | 75% | Base models (safetensors/GGUF), LoRA adapters | `CandleAdapter.model_guard`, `CandleAdapter.adapter_guards`, `GenomePagingEngine.allocation_guards` |
| **TTS** | 10% | Kokoro ONNX model, voice embeddings | `KOKORO_GPU_GUARD` static in `tts/kokoro.rs` |
| **Rendering** | 10% | Bevy render targets (16 low-res + 3 HD), VRM avatar models | `GpuGuards` Bevy Resource |
| **Reserve** | 5% | Unallocable headroom | — |

## Pressure Levels

| Range | Level | Behavior |
|-------|-------|----------|
| 0-60% | Normal | No action. All allocations succeed. |
| 60-80% | Warning | Log warnings. GenomePagingEngine evicts non-critical adapters. |
| 80-95% | High | Refuse new model loads. Aggressive LRU eviction. |
| 95%+ | Critical | Refuse all allocations. Force evictions. |

Pressure is computed as `total_used / total_budget` across all subsystems combined. A `tokio::sync::watch` channel broadcasts pressure changes to subscribers.

## RAII Allocation Guards

All GPU allocations are tracked via `GpuAllocationGuard` — an RAII struct that:
- Records the subsystem + byte count on creation
- Decrements the subsystem's `AtomicU64` usage counter on `Drop`
- Can be manually released via `.release()` for explicit lifecycle control

```rust
// Allocate
let guard = gpu_manager.allocate(GpuSubsystem::Inference, model_bytes)?;

// Guard lives in a struct field — VRAM tracked while alive
struct MyStruct {
    _model_guard: Option<GpuAllocationGuard>,
}

// When MyStruct is dropped (or guard is removed), VRAM is released automatically
```

## Per-Consumer Integration

### CandleAdapter (Inference)

**File**: `src/workers/continuum-core/src/inference/candle_adapter.rs`

- `model_guard: RwLock<Option<GpuAllocationGuard>>` — tracks base model VRAM
- `adapter_guards: RwLock<HashMap<String, GpuAllocationGuard>>` — per-LoRA adapter
- On `generate_text()` first call: lazy model load → `model.estimated_vram_bytes()` → allocate guard
- On `load_lora()`: `estimate_adapter_vram(path)` → allocate guard
- On `unload_lora()`: remove guard from map (Drop releases VRAM)
- On `shutdown()`: clear all guards

### ModelBackend Trait (VRAM Estimation)

**File**: `src/workers/continuum-core/src/inference/backends/mod.rs`

All model backends implement `estimated_vram_bytes()`:
- `LlamaSafetensorsBackend`: sums file sizes of all `weight_paths` (multiple .safetensors shards)
- `LlamaGgufBackend`: single GGUF file size via `std::fs::metadata`
- Default: 0 (unknown backend, untracked)

File size is a reasonable VRAM proxy — safetensors files are memory-mapped, GGUF includes quantized weights.

### GenomePagingEngine (Inference)

**File**: `src/workers/continuum-core/src/persona/genome_paging.rs`

- `allocation_guards: HashMap<String, GpuAllocationGuard>` — per-adapter tracking
- On `activate_skill()` eviction: `allocation_guards.remove(&victim)` releases VRAM
- On `activate_skill()` load: allocate guard for newly loaded adapter
- On `sync_state()`: re-sync guards with current adapter set
- The `memory_budget_mb` field already receives real GPU-derived budget from `CognitionState.per_persona_budget_mb()`

### Kokoro TTS

**File**: `src/workers/continuum-core/src/live/audio/tts/kokoro.rs`

- Module-level `TTS_GPU_MANAGER: OnceLock<Arc<GpuMemoryManager>>`
- `KOKORO_GPU_GUARD: OnceLock<GpuAllocationGuard>` — single ONNX model guard
- Set once during `initialize()` after ONNX session loads successfully
- Never released (TTS model is singleton, persists until process exit)

### Bevy Renderer

**File**: `src/workers/continuum-core/src/live/video/bevy_renderer.rs`

- Module-level `RENDERER_GPU_MANAGER: OnceLock<Arc<GpuMemoryManager>>`
- `GpuGuards` Bevy Resource:
  - `_render_targets: Option<GpuAllocationGuard>` — aggregate for all render textures
  - `model_guards: HashMap<u8, GpuAllocationGuard>` — per-slot VRM model
- Render target allocation at startup: `(16 × 640×360×4) + (3 × 1280×720×4)` = ~25.5MB
- VRM model allocation on `SceneInstanceReady` (file size estimate)
- Guard removed on `AvatarCommand::Unload`
- Guard replaced on `AvatarCommand::Load` (if slot already had a model)

## Observability

```bash
# Query GPU memory stats
./jtag gpu/stats

# Returns:
# {
#   gpuName: "Apple M1 Pro",
#   totalVramMb: 16384,
#   usedVramMb: 847,
#   pressure: 0.32,
#   subsystems: {
#     inference: { budgetMb: 12288, usedMb: 650 },
#     tts:       { budgetMb: 1638,  usedMb: 82  },
#     rendering: { budgetMb: 1638,  usedMb: 115 }
#   }
# }

# Query pressure level only
./jtag gpu/pressure
```

## Threading Model

The `GpuMemoryManager` is `Arc<>` shared across:
- Main tokio runtime (IPC server, persona cognition loops)
- Bevy renderer thread (via module-level `OnceLock`)
- TTS initialization (via module-level `OnceLock`)

All subsystem counters use `AtomicU64` — no mutex contention, lock-free reads/writes. The pressure watch channel uses `tokio::sync::watch` for async subscribers.

## Hardware Detection

```rust
GpuMemoryManager::detect()
```

- **macOS**: Metal API via `objc` FFI → `MTLDevice.recommendedMaxWorkingSetSize()`
- **Linux/CUDA**: Reads `nvidia-smi` output or CUDA runtime API
- **None detected**: Falls back to 0 total VRAM (all allocations succeed, no enforcement)

## Files

| File | Purpose |
|------|---------|
| `gpu/memory_manager.rs` | Core: detection, budgets, allocate/release, guards, stats |
| `gpu/mod.rs` | Module declaration |
| `modules/gpu.rs` | IPC ServiceModule: `gpu/stats`, `gpu/pressure` commands |
| `modules/cognition.rs` | `per_persona_budget_mb()` — divides inference budget by persona count |
| `inference/candle_adapter.rs` | Inference guard tracking |
| `inference/backends/mod.rs` | `ModelBackend.estimated_vram_bytes()` trait method |
| `persona/genome_paging.rs` | Genome adapter guard tracking |
| `live/audio/tts/mod.rs` | TTS GPU manager static |
| `live/audio/tts/kokoro.rs` | Kokoro ONNX model guard |
| `live/video/bevy_renderer.rs` | Renderer guard tracking (Bevy Resource) |
| `ipc/mod.rs` | Wires GPU manager to all consumers at startup |

## Related Documentation

- [RESOURCE-GOVERNANCE-ARCHITECTURE](RESOURCE-GOVERNANCE-ARCHITECTURE.md) — Five-layer resource governance stack built on this memory manager
- [LORA-GENOME-PHENOTYPES](../genome/LORA-GENOME-PHENOTYPES.md) — Genome paging concept and phenotype catalog
- [DYNAMIC-GENOME-ARCHITECTURE](../genome/DYNAMIC-GENOME-ARCHITECTURE.md) — Dynamic genome composition
- [ACADEMY-DOJO-ARCHITECTURE](../personas/ACADEMY-DOJO-ARCHITECTURE.md) — Teacher/student training pipeline
- [gpu/stats README](../../commands/gpu/stats/README.md) — CLI command docs
