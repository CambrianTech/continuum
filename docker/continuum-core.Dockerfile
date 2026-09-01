# continuum-core — Rust workers container
#
# Multi-stage build with dependency caching:
#   Stage 1 (chef):   Install cargo-chef for dependency caching
#   Stage 2 (planner): Analyze workspace, generate recipe.json (dep lockfile)
#   Stage 3 (builder): Build deps from recipe (cached), then build source
#   Stage 4 (runtime): Minimal image with just the binaries + avatar models
#
# Build context: src/workers/ (Rust workspace)
# Additional context "avatars": src/models/avatars/ (VRM files for live calls)
#
# The dep layer (~30 min) only rebuilds when Cargo.toml/Cargo.lock change.
# Source changes rebuild in ~2-3 minutes.

# ── Stage 1: Chef (dependency cache tool) ───────────────────
FROM rust:1.95-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# ── Stage 2: Plan dependencies ──────────────────────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Build ──────────────────────────────────────────
FROM chef AS builder

# System deps for compilation
# cmake + libclang needed to build vendored llama.cpp (src/workers/llama/)
# build-essential has g++/gcc for C++ compilation of llama.cpp source
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake pkg-config libssl-dev libpq-dev protobuf-compiler \
    libclang-dev clang build-essential git \
    && rm -rf /var/lib/apt/lists/*

# CUDA support (optional — only needed for GPU features)
# For CPU-only builds, skip this and don't pass --features cuda.
# For CUDA: base the builder on nvidia/cuda:devel image (separate Dockerfile variant).
ARG CUDA_VERSION=12.8
ARG GPU_FEATURES=""

# Build dependencies from recipe (CACHED — this is the big win)
# --no-default-features excludes livekit-webrtc: WebRTC is handled by the
# livekit-bridge binary (separate process, separate protobuf address space).
# load-dynamic-ort loads ONNX Runtime as shared library at runtime.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release ${GPU_FEATURES} --recipe-path recipe.json

# Now build actual source (fast — deps already compiled)
COPY . .

# entity_schemas.json is embedded at compile time by modules/entity_schemas.rs via
# include_str!("../../../../protocol/typescript/entity_schemas.json") — a source-
# relative path the `COPY . .` above already provides (the file is checked in). No
# `--from=shared*` build-context needed; models.json is unreferenced by the Rust core.

# src/shared/models.json is the model-registry SSOT. candle_adapter.rs embeds it
# via include_str!("../../../../shared/models.json"), which resolves to
# /shared/models.json from this Docker build layout.
COPY --from=shared models.json /shared/models.json

# Fail fast if the host forgot to init submodules. Without this, cmake's
# CMakeLists-not-found error surfaces ~15 min into the cargo build —
# terrible signal-to-noise. See issue #893.
RUN test -f vendor/llama.cpp/CMakeLists.txt || ( \
    echo "ERROR: vendor/llama.cpp is empty — host submodule not initialized." >&2 && \
    echo "       Run this on the host before docker build:" >&2 && \
    echo "         git submodule update --init --recursive" >&2 && \
    exit 1 )
RUN cargo build --release ${GPU_FEATURES} \
    --bin continuum-core-server \
    --bin archive-worker

# ── Stage 4: Runtime ────────────────────────────────────────
# Ubuntu 24.04 for Mesa 24+ with Vulkan dzn backend (WSL2 DirectX GPU access).
# Debian bookworm's Mesa 22 only has llvmpipe (CPU software rendering).
# Ubuntu 24.04 works on all platforms: WSL2 (dzn), Linux (nvidia/radeon), Mac (MoltenVK).
FROM ubuntu:24.04 AS runtime

# ghcr visibility default: image published to ghcr.io inherits visibility from
# the source repo when this LABEL is present. Without it, org container packages
# default to PRIVATE on first push, which blocks Carl's anonymous docker pull.
# Caught 2026-04-23: continuum-core-vulkan landed private on first push, blocked
# CI verify-architectures until visibility was manually flipped via UI.
LABEL org.opencontainers.image.source=https://github.com/CambrianTech/continuum

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3t64 libpq5 curl netcat-openbsd \
    libglib2.0-0t64 \
    libgomp1 \
    libvulkan1 mesa-vulkan-drivers \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/target/release/continuum-core-server /usr/local/bin/
COPY --from=builder /app/target/release/archive-worker /usr/local/bin/

# Model registry config — server boots with model_registry::loader reading
# /app/continuum-core/config/models.toml. Without this COPY the runtime
# panics on first start ("reading /app/continuum-core/config/models.toml:
# No such file or directory") which fails slice tests and any real use.
COPY --from=builder /app/continuum-core/config /app/continuum-core/config

# ONNX Runtime — required for Silero VAD (voice activity detection) and Piper TTS.
# These are core persona sensory capabilities (hearing + speech).
# The ort crate uses load-dynamic (dlopen), so libonnxruntime must be present at runtime.
# TARGETARCH is set by Docker BuildKit (amd64, arm64) — more reliable than uname -m
# for cross-platform builds and acts as a natural cache-bust per architecture.
ARG TARGETARCH
ARG ONNX_VERSION=1.24.4
RUN if [ "$TARGETARCH" = "arm64" ]; then \
      ORT_ARCH="linux-aarch64"; \
    else \
      ORT_ARCH="linux-x64"; \
    fi && \
    echo "TARGETARCH=$TARGETARCH → ORT_ARCH=$ORT_ARCH" && \
    curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v${ONNX_VERSION}/onnxruntime-${ORT_ARCH}-${ONNX_VERSION}.tgz" \
    | tar xz --strip-components=1 -C /usr/local \
    && ldconfig

# ort crate (load-dynamic feature) requires this env var to find the .so
ENV ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so

# Working directory — models volume mounts at /app/models so relative
# paths like "models/avatars" resolve correctly from cwd
WORKDIR /app

# Avatar VRM models — NOT baked in here. The 133MB src/models/avatars
# directory is git-ignored (matched by src/.gitignore '/models/'), so
# CI's `docker build` can't COPY them as a build context — the build
# fails with "no such file or directory: ./src/models/avatars".
#
# Live-call avatars are tracked separately as "known gap not gating
# #891" (see docs/infrastructure/PR891-E2E-VALIDATION.md). When the
# avatar-provisioning story lands (LFS, model-init download, or curl
# from a CC0 URL in CI before docker build), restore this COPY plus
# the matching `build-contexts: avatars=./src/models/avatars` lines in
# .github/workflows/docker-images.yml. Until then: empty /app/avatars
# placeholder dir so Rust catalog doesn't crash on missing path.
RUN mkdir -p /app/avatars

# Socket and data directories
RUN mkdir -p /root/.continuum/sockets /root/.continuum/jtag/data /root/.continuum/jtag/logs

# Health check — Rust core listens on its socket
HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
    CMD test -S /root/.continuum/sockets/continuum-core.sock || exit 1

# Expose socket directory as volume for IPC with node-server
VOLUME ["/root/.continuum"]

# Default: start continuum-core-server on the standard socket path.
# Override with archive-worker or other binaries as needed.
ENTRYPOINT ["continuum-core-server"]
CMD ["/root/.continuum/sockets/continuum-core.sock"]
