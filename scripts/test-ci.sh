#!/bin/bash
# Script to test CI workflow locally

set -e

echo "===== Testing CI workflow locally ====="

# Step 1: Check if we have all needed dependencies
echo "Checking dependencies..."
command -v npm >/dev/null 2>&1 || { echo "npm is required but not installed. Aborting."; exit 1; }

# Step 2: Install dependencies
echo "Installing dependencies..."
npm ci

# Step 3: Run linting
echo "Running linting..."
npm run lint

# Step 4: Run build
echo "Building packages..."
npm run build

# Step 5: Run tests
echo "Running tests..."
npm test

# Step 6: Validate schemas
echo "Validating schema..."
echo "Note: Need to install AJV CLI to run this step"
echo "npm install -g ajv-cli"
# Checking if ajv-cli is installed
if command -v ajv >/dev/null 2>&1; then
  # Validate the templates against the schema
  for template in templates/*/config.json; do
    echo "Validating $template"
    ajv validate -s schema/ai-config.schema.json -d "$template" || echo "Warning: $template failed validation"
  done
else
  echo "Warning: ajv-cli not installed, skipping schema validation"
fi

echo "===== Local CI test completed ====="