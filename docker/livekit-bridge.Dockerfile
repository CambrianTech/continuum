# livekit-bridge — WebRTC transport adapter
#
# Links webrtc-sys (LiveKit). Does NOT link ort (ONNX Runtime).
# Eliminates the protobuf symbol conflict that caused deadlocks in
# the monolithic continuum-core-server.
#
# Build context: src/workers/ (Rust workspace)

# ── Stage 1: Chef ───────────────────────────────────────────
FROM rust:1.89-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# ── Stage 2: Plan ───────────────────────────────────────────
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: Build ──────────────────────────────────────────
FROM chef AS builder

# System deps for webrtc-sys compilation
RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake pkg-config libssl-dev protobuf-compiler \
    libclang-dev clang \
    && rm -rf /var/lib/apt/lists/*

# Build dependencies from recipe (cached)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Build source
COPY . .
RUN cargo build --release --bin livekit-bridge

# ── Stage 4: Runtime ────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 curl \
    libglib2.0-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/livekit-bridge /usr/local/bin/

RUN mkdir -p /root/.continuum/sockets

HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
    CMD test -S /root/.continuum/sockets/livekit-bridge.sock || exit 1

ENTRYPOINT ["livekit-bridge"]
CMD ["/root/.continuum/sockets/livekit-bridge.sock"]
