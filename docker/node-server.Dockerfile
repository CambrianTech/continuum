# node-server — TypeScript server + orchestrator
#
# Build context: src/ (filtered by .dockerignore to ~500MB)
# Runtime: tsx executes TypeScript directly (no separate compile step needed)

FROM node:20-slim

WORKDIR /app

# Dependencies (cached layer — only rebuilds when package*.json change)
# Sharp uses prebuilt binaries — no python3/make/g++ needed
COPY package.json package-lock.json ./
RUN npm ci && npm cache clean --force

# Source code (filtered by .dockerignore)
COPY . .

# Build TypeScript (validates compilation)
RUN npm run build:ts

# Socket directory for IPC with Rust core
VOLUME ["/root/.continuum"]

EXPOSE 9000 9001

HEALTHCHECK --interval=10s --timeout=5s --start-period=30s --retries=3 \
    CMD node -e "const s=require('net').connect(9001,'localhost',()=>{s.end();process.exit(0)});s.on('error',()=>process.exit(1))"

CMD ["npx", "tsx", "server/docker-entrypoint.ts"]
