# model-init — Downloads voice, avatar, and scene models into a Docker volume.
#
# Runs once on first `docker compose up`. Volume persists across restarts.
# Re-download: docker compose run model-init
#
# Build:
#   docker build -f docker/model-init.Dockerfile -t continuum-model-init src/

FROM node:20-slim

# ghcr visibility default — see continuum-core.Dockerfile for rationale.
LABEL org.opencontainers.image.source=https://github.com/CambrianTech/continuum

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl unzip bash ca-certificates jq \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Single source of truth for ALL models the system uses (chat / vision /
# embedding / STT / TTS / VAD). Per Joel 2026-05-04:
# "we MUST have this work from ONE source of truth"
COPY shared/models.json shared/models.json
COPY scripts/download-models.sh scripts/download-models.sh
# Avatar download (VRM files) — distinct from ML models, kept separate for now.
COPY scripts/download-avatar-models.sh scripts/download-avatar-models.sh
COPY scripts/generate-scene-models.ts scripts/generate-scene-models.ts
COPY scripts/shared/ scripts/shared/
COPY package.json package.json

RUN chmod +x scripts/download-models.sh scripts/download-avatar-models.sh

ENV MODELS_DIR=/models
ENV REGISTRY=/app/shared/models.json

# Download all models from src/shared/models.json (chat-LLM tier-default,
# embeddings, STT, TTS, VAD) then avatar models. Per Joel 2026-05-04:
# "all the models must download and run on GPU" — no DMR dependency.
# continuum-core loads chat LLMs via its built-in llama.cpp + host GPU
# (Metal / CUDA / Vulkan ICD).
CMD bash scripts/download-models.sh && bash scripts/download-avatar-models.sh
