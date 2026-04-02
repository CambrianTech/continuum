# node-server — TypeScript server + orchestrator
#
# Build:
#   docker build -f docker/node-server.Dockerfile -t continuum-node src/
#
# Layer strategy:
#   1. package.json + package-lock.json → npm ci (cached until deps change)
#   2. TypeScript source → build (fast, ~20s)

FROM node:20-slim AS builder

WORKDIR /app

# Dependencies first (cached layer)
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts

# Source code
COPY . .

# Build TypeScript
RUN npm run build:ts

# ── Runtime ─────────────────────────────────────────────────
FROM node:20-slim

WORKDIR /app

COPY --from=builder /app/package.json /app/package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts

COPY --from=builder /app/dist/ ./dist/
COPY --from=builder /app/shared/ ./shared/

# Socket directory for IPC with Rust core
VOLUME ["/root/.continuum"]

# WebSocket + HTTP
EXPOSE 9000 9001

HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
    CMD curl -f http://localhost:9000/health || exit 1

CMD ["node", "dist/server-index.js"]
