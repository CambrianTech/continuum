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
