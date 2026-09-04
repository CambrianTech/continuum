# continuum-core (CUDA) — Rust workers container with NVIDIA GPU inference
#
# Extends the base continuum-core Dockerfile with CUDA toolkit for building
# our vendored llama.cpp with CUDA kernels.
#
# Build context: src/workers/ (Rust workspace, includes vendor/llama.cpp submodule)
#
# Usage in docker-compose.yml:
#   dockerfile: ../../docker/continuum-core-cuda.Dockerfile
#   args:
#     GPU_FEATURES: "--no-default-features --features load-dynamic-ort,cuda"

# ── Stage 1: Chef base (cargo-chef installed, system deps in place) ──
# Mirrors the multi-stage cargo-chef pattern from continuum-core.Dockerfile
# (CPU variant). Doing it any other way — e.g. collapsing planner+builder
# into one stage with `COPY . .` BEFORE chef cook — leaves stub binaries
# from chef-cook's recipe build in /app/target/. The subsequent
# `cargo build` then sees those stubs as "fresh" (newer mtime than the
# COPY'd source) and silently skips rebuild — producing a 436KB shell
# binary that exits 0 immediately. Painful to debug. Don't collapse.
FROM nvidia/cuda:12.8.0-devel-ubuntu22.04 AS chef

# Rust + build-time system libs. Unlike the CPU variant which uses
# rust:1.95-bookworm (Debian base with a lot of -dev libs pre-installed),
# this CUDA builder image is nvidia/cuda:...ubuntu22.04 — a minimal
# Ubuntu with just the CUDA toolchain. We need every -dev we rely on.
#
# Deps list mirrors the host dev set in src/scripts/install.sh's system
# package list so any crate that builds natively in dev also builds in
# CI. glib-sys and similar sys crates fail at `cargo chef cook` without
# their corresponding -dev packages.
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates gnupg \
    cmake pkg-config libssl-dev libpq-dev protobuf-compiler \
    libclang-dev clang build-essential git \
    libglib2.0-dev libasound2-dev libva-dev \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.95
ENV PATH=/root/.cargo/bin:$PATH
RUN cargo install cargo-chef --locked
WORKDIR /app

# ── Stage 2: Plan (read source → emit recipe.json) ──────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Build (cook deps from recipe, then real source) ──
FROM chef AS builder

# candle-kernels' build script tries to detect the CUDA compute capability
# via `nvidia-smi` at compile time. That's fine on bare metal but FAILS
# inside `docker build` — GPUs aren't exposed until `docker run --gpus all`.
# The error is: `ComputeCapDetectionFailed("Failed to run nvidia-smi: No
# such file or directory ... set CUDA_COMPUTE_CAP environment variable")`.
#
# candle-kernels accepts ONE value — not a semicolon list (tried, errored
# with `Invalid compute capability: 80;86;89;90`). We pick 90 to match
# BigMama's RTX 5090 (Blackwell). Users on older generations should
# override this at build time with `--build-arg CUDA_COMPUTE_CAP=86`
# (or whatever their arch is). Broader compat via multi-arch fat builds
# is a separate follow-up if/when we ship a generic image.
#
# Compute cap reference:
#   80 = Ampere (A100)
#   86 = Ampere (RTX 30xx, A40)
#   89 = Ada Lovelace (RTX 40xx, L40)
#   90 = Hopper / Blackwell (H100, RTX 50xx)
ARG CUDA_COMPUTE_CAP=90
ENV CUDA_COMPUTE_CAP=${CUDA_COMPUTE_CAP}

# 1. Cook deps from the recipe ONLY (no source yet → no stub binaries
#    produced for our workspace bins → no incremental-build false-positive).
ARG GPU_FEATURES="--no-default-features --features load-dynamic-ort,cuda"
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release ${GPU_FEATURES} --recipe-path recipe.json

# 2. NOW copy the real source. mtime is fresh; cargo will rebuild for real.
COPY . .

# entity_schemas.json is embedded at compile time by modules/entity_schemas.rs via
# include_str!("../../../../protocol/typescript/entity_schemas.json") — a source-
# relative path the `COPY . .` above already provides (the file is checked in). No
# `--from=shared*` build-context needed; models.json is unreferenced by the Rust core.

# Model registry SSOT used by candle_adapter.rs include_str!:
# ../../../../shared/models.json resolves to /shared/models.json here.
COPY --from=shared models.json /shared/models.json

# Fail fast if the host forgot to init submodules. Without this, cmake's
# CMakeLists-not-found error surfaces deep inside the CUDA build —
# terrible signal-to-noise. See issue #893.
RUN test -f vendor/llama.cpp/CMakeLists.txt || ( \
    echo "ERROR: vendor/llama.cpp is empty — host submodule not initialized." >&2 && \
    echo "       Run this on the host before docker build:" >&2 && \
    echo "         git submodule update --init --recursive" >&2 && \
    exit 1 )

# 3. Build the actual binaries with vendored llama.cpp CUDA kernels.
RUN cargo build --release ${GPU_FEATURES} \
    --bin continuum-core-server \
    --bin archive-worker

# ── Stage 2: Runtime (smaller, just CUDA runtime) ────────────
FROM nvidia/cuda:12.8.0-runtime-ubuntu22.04 AS runtime

# ghcr visibility default — see continuum-core.Dockerfile for rationale.
LABEL org.opencontainers.image.source=https://github.com/CambrianTech/continuum

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libpq5 curl netcat-openbsd \
    libglib2.0-0 libvulkan1 mesa-vulkan-drivers \
    libgomp1 \
    && rm -rf /var/lib/apt/lists/*
# libgomp1 is the OpenMP runtime that ggml-cpu's kernels link against
# at build time (-lgomp set in workers/llama/build.rs for Linux). Without
# it, the runtime image gets `error while loading shared libraries:
# libgomp.so.1: cannot open shared object file` and the binary refuses
# to start. CPU-only continuum-core.Dockerfile already had this baked
# into a shared lib via libglib2.0-0t64; the cuda variant uses Ubuntu
# 22.04 base which doesn't have OpenMP runtime by default.

COPY --from=builder /app/target/release/continuum-core-server /usr/local/bin/
COPY --from=builder /app/target/release/archive-worker /usr/local/bin/

# Model registry config — server boots with model_registry::loader reading
# /app/continuum-core/config/models.toml. Without this COPY the runtime
# panics on first start.
COPY --from=builder /app/continuum-core/config /app/continuum-core/config

# ONNX Runtime for Silero VAD + Piper TTS + fastembed embeddings.
#
# CRITICAL on the CUDA image: pull the `-gpu` tarball variant, not the
# CPU-only one. The GPU tarball bundles libonnxruntime_providers_cuda.so
# alongside libonnxruntime.so — without it `CUDAExecutionProvider` is
# unavailable at runtime and EVERY ORT session silently falls back to
# the MLAS CPU matmul kernels. Empirically (2026-04-24): sampled
# continuum-core during a chat-message CPU spike, 100% of hot frames
# were `MlasSgemmThreaded` in libonnxruntime — fastembed + Piper + Whisper
# + VisionDescriptionService all running on CPU despite 32GB RTX 5090
# sitting idle. Verified the shipped `.so` had zero `cuda`/`coreml`/
# `tensorrt` strings. Changing the tarball URL fixes the capability at
# runtime; additionally the Rust ORT session code must `.with_execution_
# providers([CUDAExecutionProvider::default(), ...])` to actually route
# matmul to the GPU (shipped separately — the tarball is the foundation).
#
# arm64 (linux-aarch64) has no -gpu variant from Microsoft — arm64 CUDA
# builds are Jetson-only and the community tarballs don't cover it. arm64
# here stays on the CPU-only ORT and will need a different path (TRT for
# Jetson, or skip CUDA EP) — tracked as follow-up.
ARG TARGETARCH
ARG ONNX_VERSION=1.24.4
RUN if [ "$TARGETARCH" = "arm64" ]; then \
      ORT_ARCH="linux-aarch64"; \
    else \
      ORT_ARCH="linux-x64-gpu"; \
    fi && \
    curl -fsSL "https://github.com/microsoft/onnxruntime/releases/download/v${ONNX_VERSION}/onnxruntime-${ORT_ARCH}-${ONNX_VERSION}.tgz" \
    | tar xz --strip-components=1 -C /usr/local \
    && ldconfig

ENV ORT_DYLIB_PATH=/usr/local/lib/libonnxruntime.so

WORKDIR /app
# Avatar VRM models are NOT baked in — see continuum-core.Dockerfile for
# the full reasoning. Empty placeholder dir so Rust catalog finds the path.
RUN mkdir -p /app/avatars
RUN mkdir -p /root/.continuum/sockets /root/.continuum/jtag/data /root/.continuum/jtag/logs

HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
    CMD test -S /root/.continuum/sockets/continuum-core.sock || exit 1

VOLUME ["/root/.continuum"]

ENTRYPOINT ["continuum-core-server"]
CMD ["/root/.continuum/sockets/continuum-core.sock"]
