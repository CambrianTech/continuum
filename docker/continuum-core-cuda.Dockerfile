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

WORKDIR /app

# Cache dependencies
RUN cargo install cargo-chef --locked

COPY . .
RUN cargo chef prepare --recipe-path recipe.json

ARG GPU_FEATURES="--no-default-features --features load-dynamic-ort,cuda"
RUN cargo chef cook --release ${GPU_FEATURES} --recipe-path recipe.json

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
COPY --from=avatars . /app/avatars/
RUN mkdir -p /root/.continuum/sockets /root/.continuum/jtag/data /root/.continuum/jtag/logs

HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
    CMD test -S /root/.continuum/sockets/continuum-core.sock || exit 1

VOLUME ["/root/.continuum"]

ENTRYPOINT ["continuum-core-server"]
CMD ["/root/.continuum/sockets/continuum-core.sock"]
