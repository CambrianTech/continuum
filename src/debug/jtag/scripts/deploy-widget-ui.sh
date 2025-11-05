#!/bin/bash

# JTAG Widget-UI Deployment Script
# Deploys the widget-ui example with latest JTAG package on port 9003

echo "🎭 JTAG Widget-UI Deployment Starting..."

# Navigate to widget-ui directory
cd "$(dirname "$0")/../examples/widget-ui" || {
  echo "❌ Failed to navigate to widget-ui directory"
  exit 1
}

echo "📁 Working directory: $(pwd)"

# Clean up previous installations
echo "🧹 Cleaning up previous installations..."
npm run clean:all 2>/dev/null || true

# Install/update dependencies
echo "📦 Installing dependencies with latest JTAG package..."
npm install

# Check if main JTAG system is running
echo "🔍 Checking if main JTAG system is running on port 9001..."
if ! lsof -i :9001 >/dev/null 2>&1; then
  echo "⚠️  Main JTAG system not detected on port 9001"
  echo "   Starting main JTAG system first..."
  cd ../../
  npm run system:start &
  JTAG_PID=$!
  echo "   JTAG system started with PID: $JTAG_PID"
  sleep 10  # Wait for system to initialize
  cd examples/widget-ui/
else 
  echo "✅ Main JTAG system detected on port 9001"
fi

# Check port 9003 availability
if lsof -i :9003 >/dev/null 2>&1; then
  echo "⚠️  Port 9003 is in use. Attempting to clear..."
  npx tsx ../../scripts/cleanup-dynamic-ports.ts || true
  sleep 2
fi

# Start the widget-ui server
echo "🚀 Starting Widget-UI server on port 9003..."
npm start

# The npm start will handle building and running the server
echo "✅ Widget-UI deployment complete!"
echo "🌐 Widget-UI should be available at: http://localhost:9003"
echo "🔗 Connecting to JTAG system on: ws://localhost:9001"