# continuum-core — Rust workers container
#
# Multi-stage build with dependency caching:
#   Stage 1 (chef):   Install cargo-chef for dependency caching
#   Stage 2 (planner): Analyze workspace, generate recipe.json (dep lockfile)
#   Stage 3 (builder): Build deps from recipe (cached), then build source
#   Stage 4 (runtime): Minimal image with just the binaries
#
# Build:
#   docker build -f docker/continuum-core.Dockerfile -t continuum-core src/workers/
#
# The dep layer (~30 min) only rebuilds when Cargo.toml/Cargo.lock change.
# Source changes rebuild in ~2-3 minutes.

# ── Stage 1: Chef (dependency cache tool) ───────────────────
FROM rust:1.89-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# ── Stage 2: Plan dependencies ──────────────────────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Build ──────────────────────────────────────────
FROM chef AS builder

# System deps for compilation
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake pkg-config libssl-dev libpq-dev protobuf-compiler \
    libclang-dev clang \
    && rm -rf /var/lib/apt/lists/*

# CUDA support (optional — only needed for GPU features)
# For CPU-only builds, skip this and don't pass --features cuda
ARG CUDA_VERSION=12.8
ARG GPU_FEATURES=""

# Build dependencies from recipe (CACHED — this is the big win)
# --no-default-features skips livekit-webrtc (WebRTC C++ fails on ARM64 Docker).
# LiveKit runs as its own container; Rust agent participation is desktop-only.
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --no-default-features ${GPU_FEATURES} --recipe-path recipe.json

# Now build actual source (fast — deps already compiled)
COPY . .
RUN cargo build --release --no-default-features ${GPU_FEATURES} \
    --bin continuum-core-server \
    --bin archive-worker

# ── Stage 4: Runtime ────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libpq5 curl \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries
COPY --from=builder /app/target/release/continuum-core-server /usr/local/bin/
COPY --from=builder /app/target/release/archive-worker /usr/local/bin/

# ONNX Runtime — required for Silero VAD (voice activity detection) and Piper TTS.
# These are core persona sensory capabilities (hearing + speech).
# The ort crate uses load-dynamic (dlopen), so libonnxruntime must be present at runtime.
ARG ONNX_VERSION=1.24.4
RUN curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v${ONNX_VERSION}/onnxruntime-linux-x64-${ONNX_VERSION}.tgz" \
    | tar xz --strip-components=1 -C /usr/local \
    && ldconfig

# ort crate (load-dynamic feature) requires this env var to find the .so
ENV ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so

# Working directory — models volume mounts at /app/models so relative
# paths like "models/avatars" resolve correctly from cwd
WORKDIR /app

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
