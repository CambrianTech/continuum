# continuum-core (Vulkan) — Rust workers container with GPU inference via Vulkan
#
# The Carl-on-Mac path. Apple's hypervisor does NOT expose Metal to containers
# (no IOMMU on Apple GPUs, explicit "not supported" from Docker Desktop, Apple
# container runtime, and other VM monitors). The one working path is:
#
#   Podman 4.9 + krunkit + MoltenVK
#     → container issues Vulkan API calls (via the image's libvulkan.so.1)
#     → krunkit's VMM routes them out to MoltenVK on the host
#     → MoltenVK translates Vulkan → Metal → Apple GPU
#
# llama.cpp's GGML_VULKAN backend is a first-class citizen — tested on every
# major GPU vendor, fused Vulkan shaders for quantized ops. Reference benchmark
# (M2 Max, Phi-3, llama.cpp #8042): native Metal 78 tok/s vs Vulkan-via-krunkit
# 63 tok/s → ~80% of native. Close enough that Mac Carl stays within the
# accelerated envelope, no CPU fallback required.
#
# This same image is ALSO valid on Linux hosts that expose a Vulkan device
# (Nvidia proprietary, radv/AMD, anv/Intel). It's not CUDA-specific; it's the
# generic GPU path for the project. BigMama keeps using continuum-core-cuda
# for peak throughput on the 5090.
#
# Build context: src/workers/ (Rust workspace, includes vendor/llama.cpp submodule)
#
# Usage in docker-compose (mac/vulkan profile):
#   dockerfile: ../../docker/continuum-core-vulkan.Dockerfile
#   args:
#     GPU_FEATURES: "--no-default-features --features load-dynamic-ort,vulkan"

# ── Stage 1: Chef (cargo-chef installed, system deps in place) ──
# Same multi-stage shape as the cuda variant — collapsing planner+builder
# leaves stub binaries in target/ that cargo treats as "fresh" (mtime newer
# than the later COPY .), producing a 436KB shell binary. Don't collapse.
FROM rust:1.89-bookworm AS chef

# System deps for compilation. Vulkan ICDs and shader tooling are only
# needed at runtime (runtime stage below), but we do need libvulkan-dev for
# the linker to find `-lvulkan` during `cargo build` in the builder stage.
#
# ggml-vulkan's CMakeLists.txt runs `find_package(Vulkan COMPONENTS glslc)`
# which specifically requires `glslc` (Google shaderc's SPIR-V compiler),
# NOT `glslangValidator` (which is in glslang-tools). Without glslc the
# cmake configure step bails out with:
#   Could NOT find Vulkan (missing: glslc) (found version "1.3.239")
# glslc is in its own debian package, bundled with shaderc.
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake pkg-config libssl-dev libpq-dev protobuf-compiler \
    libclang-dev clang build-essential git \
    libvulkan-dev glslc glslang-tools \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
WORKDIR /app

# ── Stage 2: Plan (read source → emit recipe.json) ──────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Build (cook deps, then real source) ─────────────
FROM chef AS builder

ARG GPU_FEATURES="--no-default-features --features load-dynamic-ort,vulkan"
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release ${GPU_FEATURES} --recipe-path recipe.json

# NOW copy real source. mtime fresh → cargo rebuilds for real.
COPY . .

# Fail fast if submodules are uninitialized.
RUN test -f vendor/llama.cpp/CMakeLists.txt || ( \
    echo "ERROR: vendor/llama.cpp is empty — host submodule not initialized." >&2 && \
    echo "       Run this on the host before docker build:" >&2 && \
    echo "         git submodule update --init --recursive" >&2 && \
    exit 1 )

RUN cargo build --release ${GPU_FEATURES} \
    --bin continuum-core-server \
    --bin archive-worker

# ── Stage 4: Runtime (Ubuntu 24.04 for Mesa 24+ with dzn / Vulkan stack) ──
# Ubuntu 24.04 ships Mesa 24.0.x: dzn backend for WSL2 DirectX, venus for
# virtio-GPU passthrough (krunkit), and working llvmpipe fallback. Debian
# bookworm's Mesa 22.x has no dzn. MoltenVK on the host side handles Mac.
FROM ubuntu:24.04 AS runtime

# Vulkan runtime + common ICDs. mesa-vulkan-drivers provides radv/venus/lvp
# which cover AMD, virtio-GPU (krunkit), and software fallback. Nvidia
# proprietary users mount their own ICD via docker run --device/--gpus.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3t64 libpq5 curl netcat-openbsd \
    libglib2.0-0t64 libgomp1 \
    libvulkan1 mesa-vulkan-drivers vulkan-tools \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/continuum-core-server /usr/local/bin/
COPY --from=builder /app/target/release/archive-worker /usr/local/bin/

# ONNX Runtime — Silero VAD + Piper TTS.
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
RUN mkdir -p /app/avatars
RUN mkdir -p /root/.continuum/sockets /root/.continuum/jtag/data /root/.continuum/jtag/logs

HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
    CMD test -S /root/.continuum/sockets/continuum-core.sock || exit 1

VOLUME ["/root/.continuum"]

ENTRYPOINT ["continuum-core-server"]
CMD ["/root/.continuum/sockets/continuum-core.sock"]
