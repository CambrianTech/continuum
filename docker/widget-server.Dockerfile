# widget-server — Browser UI
#
# Serves the Positron widget system (Lit + Shadow DOM)
# Needs full src/ context because widgets import from shared/, daemons/, commands/
#
# Build from src/:
#   docker build -f ../docker/widget-server.Dockerfile -t continuum-widgets .
#
# Run:
#   docker run -p 9003:9003 continuum-widgets

FROM node:20-slim

RUN apt-get update && apt-get install -y --no-install-recommends curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Root deps first (cached layer)
COPY package.json package-lock.json tsconfig*.json ./
RUN npm ci --ignore-scripts 2>/dev/null || npm install --ignore-scripts

# Full source (widgets reach into shared/, daemons/, commands/, scripts/)
COPY . .

# Generate shared/config.ts (required by TypeScript build — generated from config.env + package.json)
RUN npx tsx scripts/generate-config.ts 2>/dev/null || \
    node -e "const fs=require('fs'); fs.writeFileSync('shared/config.ts', \
    'export const HTTP_PORT = 9000;\nexport const WS_PORT = 9001;\nexport const ACTIVE_EXAMPLE = \"widget-ui\";\nexport const VERSION = \"1.0.0\";\nexport const config = { HTTP_PORT: 9000, WS_PORT: 9001 };\n')" || \
    echo "Config generation skipped"

# Build TypeScript
RUN npx tsc --project tsconfig.json --noEmit false --outDir dist 2>/dev/null || \
    npm run build:ts 2>/dev/null || \
    echo "TS build skipped — tsx will handle at runtime"

# Widget-ui deps + Vite bundle (must build inside Docker to pick up source changes)
RUN cd examples/widget-ui && npm install 2>/dev/null || true
RUN cd examples/widget-ui && npx vite build

EXPOSE 9003

HEALTHCHECK --interval=5s --timeout=3s --retries=3 \
    CMD curl -sf http://localhost:9003/ || exit 1

WORKDIR /app/examples/widget-ui
CMD ["npx", "tsx", "src/minimal-server.ts"]
