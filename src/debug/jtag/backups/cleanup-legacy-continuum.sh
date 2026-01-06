#!/bin/bash
# Clean up stale data from legacy .continuum/ at repo root
# Created: 2025-11-28
# Purpose: Free 36GB of disk space (Python envs, test data, external repos)

set -e

REPO_ROOT="/Volumes/FlashGordon/cambrian/continuum"

echo "🧹 Cleaning up legacy .continuum/ stale data"
echo "⚠️  This will delete 36GB of data!"
echo ""
echo "Will delete:"
echo "  - .continuum/genome/python/micromamba/  (33GB - conda env)"
echo "  - .continuum/venv/                       (2.9GB - virtualenv)"
echo "  - .continuum/shared/design-up-develop/   (923MB - external repo)"
echo "  - .continuum/genome/python/test-output/  (56MB - test logs)"
echo "  - .continuum/genome/python/*.log         (test logs)"
echo "  - .continuum/logs/                       (old logs)"
echo "  - .continuum/sessions/                   (old sessions)"
echo "  - .continuum/default/                    (legacy data)"
echo ""
read -p "Continue? (yes/no): " confirm

if [ "$confirm" != "yes" ]; then
    echo "❌ Aborted"
    exit 1
fi

cd "$REPO_ROOT"

# Check backup exists
LATEST_BACKUP=$(ls -t src/debug/jtag/backups/legacy-continuum-valuable-*.tgz 2>/dev/null | head -1)
if [ -z "$LATEST_BACKUP" ]; then
    echo "❌ ERROR: No backup found! Run backup-legacy-continuum.sh first!"
    exit 1
fi
echo "✅ Backup verified: $LATEST_BACKUP"
echo ""

# Calculate current size
echo "📊 Current size:"
du -sh .continuum/
echo ""

# Delete stale data
echo "🗑️  Deleting stale data..."

echo "  Removing micromamba env (33GB)..."
rm -rf .continuum/genome/python/micromamba/

echo "  Removing virtualenv (2.9GB)..."
rm -rf .continuum/venv/

echo "  Removing external repo (923MB)..."
rm -rf .continuum/shared/design-up-develop/

echo "  Removing test output (56MB)..."
rm -rf .continuum/genome/python/test-output/

echo "  Removing test logs..."
rm -f .continuum/genome/python/*.log

echo "  Removing old logs..."
rm -rf .continuum/logs/

echo "  Removing old sessions..."
rm -rf .continuum/sessions/

echo "  Removing legacy default directory..."
rm -rf .continuum/default/

echo ""
echo "📊 New size:"
du -sh .continuum/

echo ""
echo "✅ Cleanup complete!"
echo ""
echo "Remaining valuable data:"
ls -lah .continuum/
