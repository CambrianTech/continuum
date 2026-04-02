# model-init — Downloads voice, avatar, and scene models into a Docker volume.
#
# Runs once on first `docker compose up`. Volume persists across restarts.
# Re-download: docker compose run model-init
#
# Build:
#   docker build -f docker/model-init.Dockerfile -t continuum-model-init src/

FROM node:20-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    curl unzip bash ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy download scripts and their shared dependencies
COPY scripts/download-voice-models.sh scripts/download-voice-models.sh
COPY scripts/download-avatar-models.sh scripts/download-avatar-models.sh
COPY scripts/generate-scene-models.ts scripts/generate-scene-models.ts
COPY scripts/shared/ scripts/shared/
COPY package.json package.json

RUN chmod +x scripts/download-voice-models.sh scripts/download-avatar-models.sh

# MODELS_DIR is set by docker-compose.yml to /models (the volume mount)
ENV MODELS_DIR=/models

# Download voice models (whisper, piper, kokoro, orpheus, vad)
# then avatar models (VRM files)
# Scene generation requires tsx — skip in init, handled by npm start
CMD bash scripts/download-voice-models.sh && bash scripts/download-avatar-models.sh
