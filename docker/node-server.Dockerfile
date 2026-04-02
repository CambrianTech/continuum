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

HEALTHCHECK --interval=5s --timeout=3s --retries=5 \
    CMD node -e "fetch('http://localhost:9000/health').then(r=>{if(!r.ok)throw 1}).catch(()=>process.exit(1))"

# tsx runs TypeScript directly, handling ESM module resolution
CMD ["npx", "tsx", "server/docker-entrypoint.ts"]
