# node-server — TypeScript server + orchestrator
#
# Build:
#   docker build -f docker/node-server.Dockerfile -t continuum-node src/
#
# Layer strategy:
#   1. package.json + package-lock.json → npm ci (cached until deps change)
#   2. TypeScript source → build (fast, ~20s)
#   3. Runtime: compiled dist/ + examples + templates

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

# Compiled server
COPY --from=builder /app/dist/ ./dist/

# Runtime assets: active example serves UI to browser
COPY --from=builder /app/examples/ ./examples/
COPY --from=builder /app/templates/ ./templates/

# Socket directory for IPC with Rust core + secrets + media
VOLUME ["/root/.continuum"]

# HTTP + WebSocket
EXPOSE 9000 9001

HEALTHCHECK --interval=5s --timeout=3s --retries=5 \
    CMD node -e "fetch('http://localhost:9000/health').then(r=>{if(!r.ok)throw 1}).catch(()=>process.exit(1))"

CMD ["node", "dist/server/docker-entrypoint.js"]
