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
FROM rust:1.95-bookworm AS chef

# System deps for compilation.
#
# The two Vulkan walls we hit sequentially:
#
# 1. `find_package(Vulkan COMPONENTS glslc)` — needs glslc (Google's
#    shaderc SPIR-V compiler), not glslangValidator. Separate package.
#
# 2. ggml-vulkan.cpp uses `VK_EXT_layer_settings` which was added to
#    vulkan-hpp in 1.3.283+. Debian bookworm's libvulkan-dev is 1.3.239
#    — the shared lib works fine but the C++ headers don't know about
#    LayerSettingEXT / LayerSettingsCreateInfoEXT, so ggml-vulkan.cpp
#    fails to compile with "'LayerSettingEXT' is not a member of 'vk'".
#
# Fix (iteration 2): LunarG doesn't publish a bookworm apt repo (only
# Ubuntu codenames — jammy, noble). We can't pull their pre-built SDK
# via apt here. But the compile failure is purely a header version
# problem: ggml-vulkan.cpp needs the LayerSettingEXT symbols from
# vulkan-hpp 1.3.283+. The shared library (libvulkan.so) from bookworm's
# libvulkan-dev is linked at runtime by the ICD loader and doesn't care
# about this — runtime works fine. We only need NEWER HEADERS.
#
# Cleanest minimal fix: install Khronos Vulkan-Headers and vulkan.hpp
# directly from the KhronosGroup GitHub repo at a pinned version. They
# are header-only (~10MB), version-pinned via ARG, and install into
# /usr/local/include where CMake's FindVulkan.cmake finds them before
# the bookworm-shipped headers in /usr/include/vulkan/. libvulkan-dev
# stays installed to provide the runtime library and the .pc file.
# glslc + glslang-tools stay on debian apt packages (version-independent
# for the shader compiler).
ARG VULKAN_HEADERS_VERSION=v1.3.290
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake pkg-config libssl-dev libpq-dev protobuf-compiler \
    libclang-dev clang build-essential git ca-certificates \
    libvulkan-dev glslc glslang-tools \
    && git clone --depth 1 --branch ${VULKAN_HEADERS_VERSION} \
        https://github.com/KhronosGroup/Vulkan-Headers.git /tmp/vk-headers \
    && cmake -S /tmp/vk-headers -B /tmp/vk-headers-build \
        -DCMAKE_INSTALL_PREFIX=/usr/local \
    && cmake --install /tmp/vk-headers-build \
    && rm -rf /tmp/vk-headers /tmp/vk-headers-build \
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

# entity_schemas.json is embedded at compile time by modules/entity_schemas.rs via
# include_str!("../../../../protocol/typescript/entity_schemas.json") — a source-
# relative path the `COPY . .` above already provides (the file is checked in). No
# `--from=shared*` build-context needed; models.json is unreferenced by the Rust core.

# Model registry SSOT used by candle_adapter.rs include_str!:
# ../../../../shared/models.json resolves to /shared/models.json here.
COPY --from=shared models.json /shared/models.json

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

# ghcr visibility default — see continuum-core.Dockerfile for rationale.
LABEL org.opencontainers.image.source=https://github.com/CambrianTech/continuum

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

# Model registry config — server boots with model_registry::loader reading
# /app/continuum-core/config/models.toml. Without this COPY the runtime
# panics on first start.
COPY --from=builder /app/continuum-core/config /app/continuum-core/config

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
