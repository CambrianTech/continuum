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

# ── Stage 1: Build with CUDA toolkit ─────────────────────────
# nvidia/cuda devel image has nvcc, CUDA libs, and build tools
FROM nvidia/cuda:12.8.0-devel-ubuntu22.04 AS builder

# Rust
RUN apt-get update && apt-get install -y --no-install-recommends \
    curl ca-certificates gnupg \
    cmake pkg-config libssl-dev libpq-dev protobuf-compiler \
    libclang-dev clang build-essential git \
    && rm -rf /var/lib/apt/lists/*
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.89
ENV PATH=/root/.cargo/bin:$PATH

# candle-kernels' build script tries to detect the CUDA compute capability
# via `nvidia-smi` at compile time. That's fine on bare metal but FAILS
# inside `docker build` — GPUs aren't exposed until `docker run --gpus all`.
# The error is: `ComputeCapDetectionFailed("Failed to run nvidia-smi: No
# such file or directory ... set CUDA_COMPUTE_CAP environment variable")`.
#
# Semicolon-separated list gives us a fat binary that runs across the
# deploy targets we care about:
#   80 = Ampere (A100)
#   86 = Ampere (RTX 30xx, A40)
#   89 = Ada Lovelace (RTX 40xx, L40)
#   90 = Hopper / Blackwell (H100, RTX 50xx — BigMama is here)
# If you target a narrower range, shrink this list to cut kernel build
# time and image size.
ENV CUDA_COMPUTE_CAP=80;86;89;90

WORKDIR /app

# Cache dependencies
RUN cargo install cargo-chef --locked

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

ARG GPU_FEATURES="--no-default-features --features load-dynamic-ort,cuda"
RUN cargo chef cook --release ${GPU_FEATURES} --recipe-path recipe.json

# Fail fast if the host forgot to init submodules. Without this, cmake's
# CMakeLists-not-found error surfaces deep inside the CUDA build —
# terrible signal-to-noise. See issue #893.
RUN test -f vendor/llama.cpp/CMakeLists.txt || ( \
    echo "ERROR: vendor/llama.cpp is empty — host submodule not initialized." >&2 && \
    echo "       Run this on the host before docker build:" >&2 && \
    echo "         git submodule update --init --recursive" >&2 && \
    exit 1 )

# Build the actual binaries with vendored llama.cpp CUDA kernels
RUN cargo build --release ${GPU_FEATURES} \
    --bin continuum-core-server \
    --bin archive-worker

# ── Stage 2: Runtime (smaller, just CUDA runtime) ────────────
FROM nvidia/cuda:12.8.0-runtime-ubuntu22.04 AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 libpq5 curl netcat-openbsd \
    libglib2.0-0 libvulkan1 mesa-vulkan-drivers \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/continuum-core-server /usr/local/bin/
COPY --from=builder /app/target/release/archive-worker /usr/local/bin/

# ONNX Runtime for Silero VAD + Piper TTS
ARG TARGETARCH
ARG ONNX_VERSION=1.24.4
RUN if [ "$TARGETARCH" = "arm64" ]; then \
      ORT_ARCH="linux-aarch64"; \
    else \
      ORT_ARCH="linux-x64"; \
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
