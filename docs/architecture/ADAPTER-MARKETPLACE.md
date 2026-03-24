# Adapter Marketplace — HuggingFace as the Backbone

## The Insight

LoRA adapters published to HuggingFace already carry rich metadata from the academy training pipeline. The adapter manifest includes `traitType`, `baseModel`, `trainingMetadata.performance`, `createdAt`, and tags from the academy session. This metadata is the foundation of a zero-infrastructure marketplace.

## How It Works

Embedding that metadata and doing cosine similarity search means:

- **"I need a sprite artist for pixel games"** → find adapters tagged with `creative`, `pixel-art`, `sprite-design` that scored well
- **No need to host a marketplace server** — HuggingFace IS the host, it's already there, always online
- **Your grid nodes can be offline, your local machine can be off** — the trained expertise persists on HF
- **When you come back online**, pull the adapter, fine-tune it to your specific project aesthetic, and go

## Standardized Metadata Schema

The search could be federated — HuggingFace's own model search API supports tags and metadata filtering. We standardize the metadata schema in the adapter manifest so all Continuum-published models are discoverable:

```
continuum:role=sprite-artist
continuum:skill=pixel-art
continuum:base=qwen2.5-coder-14b
continuum:score=87
continuum:project-type=game-development
continuum:academy-session=<session-id>
continuum:persona=<persona-name>
continuum:epochs=50
continuum:rank=64
```

Then **any Continuum instance anywhere** can search "who's published a good sprite artist adapter for Qwen 14B?" and pull it down.

**Zero hosting. Zero coordination. Zero cost.** HuggingFace is the backbone.

## The Flow

```
Team trains on a project (mushroom game)
  → Sprite artist persona learns pixel art over days of academy training
  → Adapter published to HuggingFace with metadata tags
  → Months later, someone else starts a new game project
  → Their Continuum instance searches HF: "pixel art adapter, Qwen 14B, score > 80"
  → Finds the mushroom game's sprite artist adapter
  → Pulls it down, fine-tunes it for their own aesthetic
  → Skips weeks of training — standing on the shoulders of prior work
```

## Grid Integration

The grid adds another dimension:

- **Local discovery**: Grid nodes advertise their personas' adapters over Tailscale/Reticulum
- **Remote discovery**: HuggingFace search for adapters from the wider community
- **Hybrid**: Check grid first (faster, local), fall back to HF (broader, global)

A persona on someone else's grid node already learned audio engineering? Pull their adapter, compose it with your local adapters, train the delta. The grid is the local marketplace, HuggingFace is the global one.

## What Already Exists

| Component | Status | Location |
|-----------|--------|----------|
| Adapter manifest with metadata | ✅ Implemented | `AdapterManifest` in `AdapterPackageTypes.ts` |
| Training metadata (loss, epochs, performance) | ✅ Implemented | `TrainingJobEntity`, `GenomeLayerEntity` |
| Academy session tags | ✅ Implemented | `AcademySessionEntity.skill`, pipeline tags |
| HuggingFace model publishing | ✅ Proven | Published `continuum-ai/qwen2.5-coder-14b-compacted` |
| Adapter discovery (local) | ✅ Implemented | `AdapterStore.discoverForPersona()` |
| Grid node communication | ✅ Implemented | Tailscale transport, `grid/send` |
| HuggingFace search API | 🔲 Not yet wired | HF API supports tag filtering natively |
| Standardized tag schema | 🔲 Not yet defined | Needs `continuum:*` tag convention |
| `genome/adapter-publish` command | 🔲 Not yet built | Would push adapter + manifest to HF |
| `genome/adapter-search` command | 🔲 Not yet built | Would search HF + grid for matching adapters |
| `genome/adapter-pull` command | 🔲 Not yet built | Would download + register adapter from HF |

## Why This Matters

This is how specialized AI agents become a shared resource. Training is expensive — days of GPU time on a 5090. But once trained, that expertise is a permanent asset that anyone can build on. The adapter marketplace turns individual training runs into a commons. Every team project that publishes its adapters makes the next team's project faster.

Not a walled garden. Not a subscription. Not a centralized service. Just LoRA adapters with good metadata on a platform that already exists.
