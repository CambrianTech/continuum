# Real-Time Architecture Principles

## Core Philosophy

**Never use a resource unless necessary.**

This single rule, applied rigorously, produces systems that scale from iPhone 7s to data centers. Every allocation, every copy, every synchronization point is a decision - not a default.

---

## The CBARFrame Pattern

From augmented reality systems running 60fps computer vision on mobile:

```
Frame arrives from camera
    ↓
┌─────────────────────────────────────────────────────────┐
│  GPU Pipeline (texture never leaves GPU)                │
│                                                         │
│  textureId → grayscale filter → attach to frame         │
│           → optical flow      → attach to frame         │
│           → feature extract   → attach to frame         │
│           → pose estimation   → attach to frame         │
└─────────────────────────────────────────────────────────┘
    ↓
Frame travels through system with ALL computed data attached
    ↓
Any consumer reads what they need - zero recomputation
```

**Key insight**: The frame is a container for metadata. Raw pixels stay on GPU. Computed features attach once and travel forever.

---

## Resource Hierarchy

### 1. GPU Memory (Most Precious)
- Textures stay as texture IDs
- Tensors stay on device
- Only metadata crosses GPU↔CPU boundary

### 2. CPU Memory
- Preallocated pools (RTOS-style recycling)
- Box<Vec> with known capacity
- Ring buffers for streaming
- Zero-copy slices where possible

### 3. Disk/Network
- Memory-mapped files (safetensors pattern)
- Lazy loading - only fault in pages accessed
- Streaming protocols - never buffer entire payload

### 4. Compute Time
- Adaptive priorities per thread/process
- Work-stealing for load balancing
- Deadline-aware scheduling

---

## Adaptive Priority System

Like a CPU OS scheduler, but for AI workloads:

```
┌─────────────────────────────────────────────────────────┐
│                 Priority Scheduler                       │
├─────────────────────────────────────────────────────────┤
│  CRITICAL   │ Frame decode, audio sync, user input      │
│  HIGH       │ Inference for active conversation         │
│  NORMAL     │ Background embedding, indexing            │
│  LOW        │ Training, consolidation, cleanup          │
│  IDLE       │ Speculative precomputation                │
└─────────────────────────────────────────────────────────┘

Priorities are ADAPTIVE:
- User looking at chat? Chat inference → CRITICAL
- User in video call? Frame processing → CRITICAL, chat → LOW
- System idle? Training → HIGH (opportunistic)
```

**AI-Assisted Prioritization**: The system can use lightweight models to predict what the user needs next, promoting those tasks preemptively.

---

## Zero-Copy Patterns

### Pass Handles, Not Data

```rust
// ❌ WRONG - copies data
fn process(data: Vec<u8>) { ... }

// ✅ RIGHT - borrows slice
fn process(data: &[u8]) { ... }

// ✅ BETTER - passes handle, data stays on GPU
fn process(texture_id: GpuTextureId) { ... }
```

### Attach Results, Don't Return Them

```rust
// ❌ WRONG - allocates new struct, copies results
fn compute_features(frame: &Frame) -> Features { ... }

// ✅ RIGHT - mutates in place, no allocation
fn compute_features(frame: &mut Frame) {
    frame.features = Some(compute_on_gpu(frame.texture_id));
}
```

### Ring Buffers for Streaming

```rust
struct FrameRing {
    frames: Box<[Frame; 60]>,  // Fixed allocation
    write_idx: AtomicUsize,
    read_idx: AtomicUsize,
}

// Producer writes to next slot (no allocation)
// Consumer reads from current slot (no copy)
// Old frames recycled automatically
```

---

## Bottleneck Elimination

### Identify Bottlenecks

1. **Synchronization** - Locks, mutexes, barriers
2. **Allocation** - malloc/free, GC pauses
3. **Copies** - memcpy, serialization, GPU↔CPU transfers
4. **I/O Blocking** - Disk, network, device access

### Eliminate Each

| Bottleneck | Solution |
|------------|----------|
| Locks | Lock-free queues, message passing |
| Allocation | Object pools, arena allocators |
| Copies | Handles, slices, memory mapping |
| I/O Blocking | Async I/O, io_uring, completion ports |

---

## Multi-Modal Pipeline Design

For video, audio, images - the principles scale:

```
┌─────────────────────────────────────────────────────────┐
│                    Media Pipeline                        │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Video Stream                                           │
│    └→ Decode (GPU) → Frame pool (recycled)             │
│         └→ Attach: pose, expression, embedding          │
│              └→ Route to consumers (zero-copy refs)     │
│                                                         │
│  Audio Stream                                           │
│    └→ Decode → Ring buffer (fixed allocation)          │
│         └→ Attach: transcription, emotion, speaker_id   │
│              └→ Route to consumers                      │
│                                                         │
│  AI Thoughts                                            │
│    └→ Never block media pipeline                       │
│    └→ Attach as metadata when ready                    │
│    └→ Adaptive priority based on context               │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

**Critical Rule**: AI processing NEVER blocks media pipelines. Thoughts attach asynchronously as metadata - if inference is slow, the frame just travels without that annotation.

---

## Rust Enables This

The ownership model forces discipline:

```rust
// Can't accidentally copy - must explicitly clone
let frame = Frame::new();
process(frame);  // Moves ownership
// frame is gone - can't accidentally reuse

// Borrow checker enforces single-writer
let mut frame = Frame::new();
compute_features(&mut frame);  // Exclusive access
attach_embedding(&mut frame);  // Sequential, safe
// No data races possible
```

**Memory-mapped safetensors**: Model weights are never "loaded" - they're mapped into address space. OS handles paging. Only accessed weights fault into RAM.

**GPU tensors**: Candle keeps tensors on Metal/CUDA. Forward pass happens entirely on device. Only final logits cross to CPU for sampling.

---

## Implementation Checklist

When adding any new component:

- [ ] What resources does it need? (GPU, CPU, disk, network)
- [ ] Can it work with handles instead of data?
- [ ] Does it need to allocate, or can it use a pool?
- [ ] Does it block? Can it be async?
- [ ] What's its priority? Is it adaptive?
- [ ] Does it attach results or return them?
- [ ] Where are the potential bottlenecks?

---

## Future: Sora-Like Video Generation

For real-time avatar/video generation:

```
User Intent (text/voice)
    ↓
Lightweight intent model (CRITICAL priority)
    ↓
Frame generation request (metadata only)
    ↓
┌─────────────────────────────────────────────────────────┐
│  Video Generation Pipeline (all GPU)                    │
│                                                         │
│  Diffusion steps happen on device                       │
│  Output: texture_id (not pixels)                        │
│  Compositor takes texture_id directly                   │
│  Pixels never touch CPU until final display             │
└─────────────────────────────────────────────────────────┘
    ↓
Display (texture_id → screen, zero-copy on Apple/Vulkan)
```

The entire pipeline is metadata and handles until the final blit to screen.

---

## Summary

**Elegant design = absence of waste**

- No unnecessary allocations
- No unnecessary copies
- No unnecessary synchronization
- No unnecessary blocking

Every resource use is intentional. Every priority is adaptive. Every bottleneck is eliminated through design, not brute force.

The system should feel like water flowing downhill - no friction, no resistance, just natural movement from input to output.
