# Live Avatar Budget — 14 citizens on camera without eating the machine

**Status:** core requirements (2026-08-31, Joel: "how do we handle 14,
30fps avatars in video without eating resources… take every advantage").
Binds the renderer law (hot potato: pixels never touch CPU) and the
serving reality: **the same GPU is decoding a 35B LLM, and the LLM's
token latency is the product** (act-latency law). The acceptance bar is
the standing one: **14 personas live on an M1 32GB** — M5 Pro is the
comfortable case, not the target.

## 1. Do the math before the architecture (or fear the wrong thing)

Per avatar at 640×360 NV12, 30fps:
- Render + convert traffic ≈ 230K px × (RGBA write + read + NV12 write)
  ≈ ~1.6MB/frame ≈ 48MB/s. ×14 ≈ **0.7GB/s** against ~200GB/s (M1 Pro)
  to ~270GB/s unified-memory bandwidth: **<0.5%. Fill and bandwidth are
  NOT the fight.**
- The real costs, in order:
  1. **Encoder sessions** — 14 simultaneous H.264/VP8 encodes.
  2. **GPU scheduling jitter** — render command buffers interleaving with
     Metal LLM decode kernels (decode is bandwidth-bound and
     latency-sensitive; a long render pass stalls a token batch).
  3. **CPU orchestration wakeups** — 14 × 30 = 420 dispatch/signal cycles
     per second if we're naive about ticks.
  4. **Skinning/animation** — 14 VRM skeletons.

## 2. Every advantage (the inventory — use ALL of them)

1. **The media engine encodes for free.** VideoToolbox is a dedicated
   hardware block — neither GPU nor CPU. 14×360p30 is a fraction of what
   it does (it chews multi-stream 4K). Law: **never software-encode;
   never encode on the GPU.** The NV12 IOSurface → VideoToolbox path we
   built is the only path.
2. **Unified memory + IOSurface** — zero-copy end to end (already law;
   `metal_gpu_convert.rs` is the receipt).
3. **Nobody needs 30fps while idle.** An idle avatar (breath, blink) is
   perceptually perfect at 10–12fps; **30fps is reserved for SPEAKING
   (visemes) and gesturing** — and we own the speaking signal (the TTS
   stream lifecycle). Frame pacing is per-slot state, not global.
4. **Don't render the unwatched.** LiveKit's adaptive streams tell us who
   is actually subscribed/visible. No subscriber → 1fps keepalive (or
   hold the last surface). The camera nobody watches costs ~nothing.
   This is attention-based foveation at the SYSTEM level.
5. **Content-addressed frame skip at the source.** A sleeping persona's
   animation state is ~constant: if the pose/viseme/emotion inputs hash
   equal, don't re-render — resend the last IOSurface (the capture pipe
   already hashes JPEGs; move the dedup BEFORE the render).
6. **One app, shared everything.** 14 cameras in ONE Bevy world: shared
   scene assets, shared pipelines/materials (VRMs instanced where
   possible), skinning on GPU. Never 14 processes, never 14 worlds.
7. **Small command buffers, paced ticks.** Avatar render work submits in
   small chunks so llama's Metal kernels never wait behind a monolithic
   pass; the Bevy tick is capped (15Hz base / 30Hz speaking-slots-only),
   never free-running. Stretch: pace render submissions off the serving
   daemon's decode-batch boundary (it knows the cadence) so render slots
   into decode gaps instead of colliding.
8. **Resolution ladder.** 360p is the roster/grid truth; only the FOCUSED
   speaker earns 720p. Simulcast layers come from the encoder, not from
   rendering twice.
9. **The ortho/home cameras ride the same budget** — a room camera is one
   more slot obeying the same pacing rules (watched → fps, unwatched →
   keepalive). The neighborhood at rest costs a keyframe.
10. **Batch the wakeups.** One orchestration tick services ALL slots due
    that tick (single command buffer submit, single signal wave) — 30
    wakeups/s total, not 420.

## 3. The budget, stated as acceptance receipts

- **B1**: 14 idle avatars live: total GPU time for render+convert ≤ 3%
  of frame budget on M1 Pro; **decode tok/s p50 unchanged within noise
  (±2%)** vs avatars-off — measured with the existing per-slot throughput
  verdict probes. This is the receipt that matters; if avatars tax
  tokens, the pacing failed.
- **B2**: 2 speaking + 12 idle: speakers at 30fps with lip-sync, idlers
  at ≤12fps, CPU orchestration <2% of one core (420-wakeup naive mode is
  a fail even if it "works").
- **B3**: zero per-frame heap allocations on the live path (instruments
  trace), zero CPU pixel touches (the hot-potato audit question).
- **B4**: unwatched tiles (no LiveKit subscriber) render ≤1fps.
- **B5**: all of the above simultaneously with a benchmark round decoding
  — the live plane and the work plane share the machine by DESIGN, not by
  luck.

## 4. What we refuse (the cheapness list)

- No software encoders, no GPU encode compute, no per-avatar processes,
- no free-running render loops, no fixed-30fps-for-everyone,
- no CPU pixel conversion "just for this path",
- no second renderer for the ortho views,
- no per-frame `to_vec()` — the gpu_bridge doc already names the corpses.
