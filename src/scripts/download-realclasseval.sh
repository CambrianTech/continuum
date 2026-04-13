#!/bin/bash
# Downloads the RealClassEval dataset (MIT license)
# Paper: https://arxiv.org/abs/2510.26130
# Repo: https://github.com/mrsumitbd/RealClassEval-Replication
#
# Usage:
#   bash scripts/download-realclasseval.sh [destination]
#
# After download, import with:
#   ./jtag genome/dataset-import --source=realclasseval

set -euo pipefail

DEST="${1:-.continuum/datasets/realclasseval-raw}"
REPO_URL="https://github.com/mrsumitbd/RealClassEval-Replication"

echo "📦 Downloading RealClassEval dataset..."
echo "   Destination: $DEST"

if [ -d "$DEST" ]; then
  echo "   Directory already exists. Pulling latest..."
  cd "$DEST" && git pull --ff-only
else
  echo "   Cloning repository (shallow)..."
  git clone --depth 1 "$REPO_URL" "$DEST"
fi

# Verify expected structure exists
DATA_DIR="$DEST/data/functional_correctness_data"
if [ ! -d "$DATA_DIR" ]; then
  echo "❌ Expected directory not found: $DATA_DIR"
  echo "   The repository structure may have changed. Check: $REPO_URL"
  exit 1
fi

# Count entries per split
CSN_CSV="$DATA_DIR/csn/dfs/no_docstr.csv"
POST_CSV="$DATA_DIR/post_cut-off/dfs/no_docstr.csv"

CSN_COUNT=0
POST_COUNT=0

if [ -f "$CSN_CSV" ]; then
  CSN_COUNT=$(tail -n +2 "$CSN_CSV" | wc -l | tr -d ' ')
fi

if [ -f "$POST_CSV" ]; then
  POST_COUNT=$(tail -n +2 "$POST_CSV" | wc -l | tr -d ' ')
fi

TOTAL=$((CSN_COUNT + POST_COUNT))

echo ""
echo "✅ RealClassEval dataset ready:"
echo "   Pre-cutoff (CSN): $CSN_COUNT classes"
echo "   Post-cutoff:      $POST_COUNT classes"
echo "   Total:            $TOTAL classes"
echo ""
echo "Import with:"
echo "   ./jtag genome/dataset-import --source=realclasseval"
