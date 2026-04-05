# node-server — TypeScript server + orchestrator
#
# Build:
#   docker build -f docker/node-server.Dockerfile -t continuum-node src/
#
# Layer strategy:
#   1. package.json + package-lock.json → npm ci (cached until deps change)
#   2. Source code copied last (changes most often)
#
# Uses tsx at runtime because tsc emits ES2020 imports without .js extensions,
# which Node 20 can't resolve natively. tsx handles this transparently.

FROM node:20-slim

WORKDIR /app

# Dependencies (cached layer — only rebuilds when package*.json change)
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts

# Source code
COPY . .

# Build TypeScript (validates compilation, generates dist/ for any direct node usage)
RUN npm run build:ts

# Runtime assets: examples serve UI to browser
# (already in source tree from COPY . .)

# Socket directory for IPC with Rust core + secrets + media
VOLUME ["/root/.continuum"]

# HTTP + WebSocket
EXPOSE 9000 9001

# Health check via WebSocket port (HTTP is skipped in Docker — widget-server handles it).
# Uses a TCP connect check since WebSocket upgrade requires a client library.
# Start period gives the Node.js server time to bootstrap (344 commands, 17 daemons).
HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=3 \
    CMD node -e "const s=require('net').connect(9001,'localhost',()=>{s.end();process.exit(0)});s.on('error',()=>process.exit(1))"

# tsx runs TypeScript directly, handling ESM module resolution
CMD ["npx", "tsx", "server/docker-entrypoint.ts"]
