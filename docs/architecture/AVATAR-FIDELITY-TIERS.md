# Avatar fidelity tiers: photoreal faces as an elastic quality of service on the grid

Status: design direction (Joel's research, 2026-11-22), not built. Numbers from the research are
labelled **(research claim)** until measured on our nodes. It extends `LIVE-AVATAR-BUDGET.md` (raw notes;
its laws hold: pixels never touch the CPU, and the same GPU decodes the LLM, so a render must never stall
a token) and feeds the consolidated live-plane design.

## The idea

No actor, no capture studio, no fixed VRAM budget. Each persona's face is synthesized once and animated
for as long as she lives. Visual fidelity is a quality-of-service tier the grid picks per node from what
that node can do right now, and it moves when the node gets busier or freer, as every other placement
does (`GRID-ELASTIC-CAPABILITY.md`).

## 1. Identity, made once

- **One portrait and one voice seed.** A single high-detail reference portrait (generated, or rendered
  from a base mesh) paired with her voice (zero-shot clone or an open-weight TTS voice). The portrait and
  voice are part of her genome, versioned like her other traits.
- **Audio-driven neural warp** (LivePortrait, Ditto, JoyVASA style): a small motion predictor, about
  50-150M parameters **(research claim)**, maps streaming audio features to head pose, lips, blinks and
  micro-expressions, and warps the one portrait at 30-60 fps in under 1.5 GB VRAM **(research claim)**.
- **A baked 3DGS head, still no actor**: generate a short talking clip of her portrait once with a heavy
  video model on the 5090, then train a 3D Gaussian Splatting head on those synthetic frames. Rendering
  after that is rasterization, 100+ fps on Metal or CUDA with almost no neural compute **(research claim)**.
  This is the high tier where it fits: the model work happens once, offline.

## 2. Tiers, picked per node by the grid

| Tier | Node | Voice | Face |
|---|---|---|---|
| 1 | 5090, M5 Pro | full-duplex local speech, local reasoning | neural warp or 3DGS at 60 fps, driven from local audio latents |
| 2 | M1/M2, 1080 Ti class | local streaming ASR/TTS, heavy reasoning on the grid | quantized motion net (FP16/INT8) at 512x512, upscaled locally, 24-30 fps |
| 3 | constrained edge | small quantized audio model or grid-streamed audio | viseme/expression state machine blending 8-12 pre-rendered keyframe loops |
| 4 | thin client beside a strong grid node | local mic/speaker ring buffer | motion latents streamed from the grid node, warped locally (section 3) |

The tier is a node capability the grid already negotiates, not a setting. A node publishes what it can
render (its measured warp and splat rates, its encoder sessions, its spare decode budget) as state;
placement reads it. When the LLM on that node needs the GPU, the face drops a tier before a token waits.

## 3. Stream motion, never video

A weak node that cannot run the motion model never receives video. A grid node runs audio-to-motion and
sends the motion itself: about 64 floats of pose and expression per frame at 30 Hz, under 10 KB/s
**(research claim)**; the client runs only the small warp decoder or blends morph targets.

This is the binary-stream rule applied to faces:
- **Binary frames, never text.** A motion frame is fixed-layout floats: a straight copy end to end.
- **One media timeline.** Motion, audio and video are streams in one frame format with kind, stream
  id, timestamp and sequence (the follow-up named in PR #6230), so the client aligns lips to audio by
  timestamp, as any player does.
- **UDP-style delivery.** A late motion frame is worse than a lost one: the newest frame wins, gaps are
  visible in the sequence, nothing retransmits stale motion.

## 4. Where small models are safe, and where they are not

- **Small is fine for:** blink and expression prediction, nods and active-listening cues, viseme
  extraction, background summarization. Conditioned on prosody, 20M-500M parameters reads as alive
  **(research claim)**.
- **Never skimp on the voice.** People forgive a stylized or low-resolution face at once; robotic cadence
  or clumsy turn-taking breaks the illusion. The best compute on a node goes to the full-duplex audio and
  prosody loop.

## 5. How it fits the engine we are building

- **Every piece is a SAGE component on its own cadence** (SAGE.md): the motion predictor hears her
  audio stream and publishes motion frames; the renderer watches the newest motion state; the encoder
  takes rendered frames. None waits on another; a slow renderer drops to the newest motion, never stalls
  her voice.
- **The GPU is shared with decode.** The lane (SAGE.md, "The lane") owns decode steps; renders interleave
  between them on the measured per-step budget. The 2026-11-22 batching measurement (CONTINUUM-ENGINE
  4.1b') found about 10.8 ms of fixed per-step cost in decode; the render's slot sits beside that, and a
  render that would push a decode step past its bound drops a tier instead.
- **The tier policy learns like the slice**: a control loop on measured frame times and decode tok/s per
  node, never a hand table, with the table above as its starting point.

## Open, to measure first

1. LivePortrait-class warp on the M5 Pro and the 5090 with the 35B decoding beside it: fps, VRAM, and the
   decode tok/s it costs.
2. A 3DGS head trained from a synthetic clip: training time on the 5090, render fps on each node.
3. Motion-latent size and the warp decoder's cost on a weak client (the IntelMac, the 1080 Ti box).
4. Which runtime hosts the motion model in-process (Candle, ONNX Runtime, our own Metal/CUDA path):
   decided by measured latency on two outlier nodes, per the methodical process.
