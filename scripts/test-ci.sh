#!/bin/bash
# Script to test CI workflow locally

set -e  # Exit on first error

echo "===== Testing CI workflow locally ====="

# Step 1: Check if we have all needed dependencies
echo "Checking dependencies..."
command -v npm >/dev/null 2>&1 || { echo "npm is required but not installed. Aborting."; exit 1; }

# Step 2: Install dependencies
echo "Installing dependencies..."
npm ci || npm install

# Step 3: Configure environment variables (same as CI)
echo "Configuring environment variables..."
export JEST_WORKER_ID=1
export NODE_OPTIONS=--experimental-vm-modules

# Step 4: Run linting
echo "Running linting..."
npm run lint

# Step 5: Run build
echo "Building packages..."
npm run build

# Step 6: Run tests
echo "Running tests..."
npm test

# Step 7: Validate schemas
echo "Validating schema..."
# Install ajv-cli locally if needed
npm install --save-dev ajv-cli
npx ajv-cli validate -s schema/continuum.schema.json -d "templates/*/config.json"

# Step 8: Run example
echo "Running example..."
if [ -f "examples/visualize-config.js" ] && [ -f "examples/.continuum/default/config.md" ]; then
  cd examples
  node visualize-config.js .continuum/default/config.md
  cd ..
fi

echo "✅ All CI checks passed locally!"