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

## The other road: one model for voice and face (Nuance Labs' Nuplex)

Nuance Labs (founders from Apple's Vision Pro Personas) runs a single full-duplex autoregressive
transformer that predicts audio and visual tokens together, decoded straight to frames, with no rig, mesh
or game engine. Its output is photoreal at about 500 ms latency **(research claim; seen in a public clip,
2026-11-22)**. The visual tokenizer and decoder are unpublished.

What it teaches, and why the split pipeline stays our default:
- **Voice and face from one source** is why its micro-expressions land: nothing is stitched after the
  fact. We take the principle cheaply. The motion model is driven by the voice model's own internal
  latents, not by the finished audio, so the face moves with the intent of the speech at a few KB/s.
- **Autoregressive video drifts.** The public clip's own caption: "it gets worse as it goes". Each frame
  is predicted from the frames before it, so error compounds over a long take. Minds here talk for hours
  and live for months. The split pipeline renders every frame from one fixed identity (portrait or baked
  3DGS head); only motion changes, so identity cannot drift.
- **Cost on our hardware.** A second large transformer per mind, predicting visual tokens at video rate,
  competes with reasoning for the same memory bandwidth. The 2026-11-22 profile found decode already 97%
  GPU-busy (CONTINUUM-ENGINE 4.1b'). The token rate per second depends on the tokenizer and is unmeasured
  here.
- **500 ms reads as natural** in full duplex, as long as backchannels (a nod, "mm", a breath) arrive
  faster. That holds for either road.

**Possible top tier:** an end-to-end audiovisual model on the 5090 for one persona at a time, if open
weights with real-time decode appear. It is measured against the split pipeline on drift over a long
take, latency, and decode tok/s lost by the reasoning model beside it.

## The first stack (research, 2026-11-22; every claim from the projects' own pages, unmeasured here)

**Slice one: LivePortrait renderer driven by Ditto's audio-to-motion model.**
- **One motion space.** Ditto (Ant Group, code Apache-2.0) is a diffusion transformer over LivePortrait's
  own motion: 265 dimensions per frame (63-D expression = 21 keypoints x xyz, plus rotation and
  translation). That vector is also the wire format for weak clients: about 13 KB/s at fp16, 25 fps.
- **Identity is free.** Her portrait goes through LivePortrait's appearance and motion extractors once,
  offline; no training step, no actor.
- **Already half ours.** Ditto ships ONNX weights for the motion model (`lmdm_v0.4_hubert.onnx`) and a
  streaming HuBERT with a KV cache (`hubert_streaming_fix_kv.onnx`): in-process from Rust. Its first frame
  after 385 ms and real-time factor 0.895 were measured on one A100.
- **The renderer is small.** LivePortrait (KwaiVGI, MIT) is about 130M parameters, about 15 ms a frame on
  a 4090: warp 45.5M / 5.2 ms, SPADE generator 55M / 7.6 ms. On a Mac through PyTorch MPS it is
  "maybe 20x slower", so it runs on our own Metal kernels; its 3D grid-sample warp is the hotspot (the same
  fusion and kernel work as decode, CONTINUUM-ENGINE 4.1b').

**Slice two: a 3DGS head for phones and weak clients.** Alibaba's LAM builds an animatable Gaussian head
from one image in about 1.4 s, no per-identity training, Apache-2.0, 110+ fps on a phone, driven by
LAM_Audio2Expression ARKit blendshapes. It waits on an open question about its FLAME and training-data
provenance. The open audio-driven 3DGS heads (TalkingGaussian, GaussianTalker, GaussianAvatars) need actor
video and per-identity training under research or non-commercial licences: excluded.

**Licences to clear before any commercial use:**
1. InsightFace detectors bundled with LivePortrait and Ditto are non-commercial. Detection runs once per
   generated portrait, so the bundled BlazeFace/face-mesh replace them.
2. Ditto's weight licence is not stated apart from its Apache-2.0 code: ask Ant Group.
3. FLAME: only FLAME 2023 Open is CC-BY-4.0.

**Voice latents.** Every open motion model takes HuBERT or Wav2Vec features (about 50 Hz). Driving the face
from a speech model's own latents (Moshi's Mimi at 12.5 Hz; NVIDIA PersonaPlex, a 7B model on Moshi,
commercial under the NVIDIA Open Model License) means retraining Ditto's motion head on them. That is ours
to do, since we train genes anyway, and it is how voice and face come from one source.

**Excluded as offline or partial:** Hallo2 (SD1.5 plus AnimateDiff), MuseTalk (mouth inpainting only),
JoyVASA (real time is future work), FLOAT (non-commercial), NVIDIA Audio2Face-3D (CUDA and mesh-vertex
output only: a 5090 option for a mesh rig).

## Open, to measure first

1. LivePortrait-class warp on the M5 Pro and the 5090 with the 35B decoding beside it: fps, VRAM, and the
   decode tok/s it costs.
2. A 3DGS head trained from a synthetic clip: training time on the 5090, render fps on each node.
3. Motion-latent size and the warp decoder's cost on a weak client (the IntelMac, the 1080 Ti box).
4. Which runtime hosts the motion model in-process (Candle, ONNX Runtime, our own Metal/CUDA path):
   decided by measured latency on two outlier nodes, per the methodical process.
