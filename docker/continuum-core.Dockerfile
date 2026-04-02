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
FROM rust:1.87-bookworm AS chef
RUN cargo install cargo-chef
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
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release ${GPU_FEATURES} --recipe-path recipe.json

# Now build actual source (fast — deps already compiled)
COPY . .
RUN cargo build --release ${GPU_FEATURES} \
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

# Copy ONNX runtime if built with it
# The load-dynamic feature means we need the .so at runtime
COPY --from=builder /app/target/release/build/ort-*/out/onnxruntime-*/lib/*.so* /usr/local/lib/ 2>/dev/null || true
RUN ldconfig

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

# Default: start continuum-core-server
# Override with archive-worker or other binaries as needed
ENTRYPOINT ["continuum-core-server"]
