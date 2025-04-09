#!/bin/bash
# Script to safely update Lerna from v6 to v8

set -e  # Exit on first error

echo "===== Updating Lerna to v8 ====="

# Step 1: Check current node version
NODE_VERSION=$(node -v)
echo "Using Node.js version: $NODE_VERSION"

# Check if node version is at least 18
if [[ ${NODE_VERSION:1:2} -lt 18 ]]; then
  echo "ERROR: Node.js version 18 or higher is required for Lerna v8"
  echo "Current version: $NODE_VERSION"
  echo "Please update Node.js and try again"
  exit 1
fi

# Step 2: Update Lerna
echo "Installing Lerna v8..."
npm install --save-dev lerna@^8.0.0

# Step 3: Run Lerna repair to fix configuration
echo "Running lerna repair to update configuration..."
npx lerna repair

# Step 4: Update postinstall script
echo "Updating postinstall script in package.json..."
# This is typically done manually or with a package.json editor
# For this script, we'll just inform the user
echo "IMPORTANT: If you encounter errors, update the postinstall script:"
echo "  From: \"postinstall\": \"lerna bootstrap\""
echo "  To:   \"postinstall\": \"lerna run prepare\""

echo "===== Lerna update completed ====="
echo "Please test your monorepo workflow to ensure everything works as expected"