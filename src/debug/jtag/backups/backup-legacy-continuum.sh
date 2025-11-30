#!/bin/bash
# Backup valuable data from legacy .continuum/ at repo root
# Created: 2025-11-28
# Purpose: Preserve docs/configs before deleting 36GB of stale data

set -e

REPO_ROOT="/Volumes/FlashGordon/cambrian/continuum"
BACKUP_DIR="/tmp/legacy-continuum-backup"
TIMESTAMP=$(date +%Y%m%d-%H%M%S)
BACKUP_FILE="$REPO_ROOT/src/debug/jtag/backups/legacy-continuum-valuable-$TIMESTAMP.tgz"

echo "🗄️  Backing up valuable data from legacy .continuum/"

# Clean previous backup temp dir
rm -rf "$BACKUP_DIR"
mkdir -p "$BACKUP_DIR"

cd "$REPO_ROOT"

# Backup documentation (incident reports, architecture docs)
echo "📄 Backing up documentation..."
mkdir -p "$BACKUP_DIR/share"
cp -v .continuum/share/*.md "$BACKUP_DIR/share/" 2>/dev/null || true

# Backup genome scripts and configs (NOT the 33GB micromamba env!)
echo "🧬 Backing up genome scripts and configs..."
mkdir -p "$BACKUP_DIR/genome-scripts"
cp -v .continuum/genome/python/*.sh "$BACKUP_DIR/genome-scripts/" 2>/dev/null || true
cp -v .continuum/genome/python/*.yml "$BACKUP_DIR/genome-scripts/" 2>/dev/null || true
cp -v .continuum/genome/python/*.md "$BACKUP_DIR/genome-scripts/" 2>/dev/null || true
cp -v .continuum/genome/python/*.txt "$BACKUP_DIR/genome-scripts/" 2>/dev/null || true

# Backup session learnings and analysis
echo "📊 Backing up session data..."
cp -v .continuum/*.json "$BACKUP_DIR/" 2>/dev/null || true
cp -v .continuum/*.txt "$BACKUP_DIR/" 2>/dev/null || true
cp -v .continuum/*.sh "$BACKUP_DIR/" 2>/dev/null || true

# Create tarball
echo "📦 Creating backup archive..."
cd /tmp
tar czf "$BACKUP_FILE" legacy-continuum-backup

# Verify backup
BACKUP_SIZE=$(du -sh "$BACKUP_FILE" | awk '{print $1}')
echo "✅ Backup created: $BACKUP_FILE ($BACKUP_SIZE)"

# Cleanup temp dir
rm -rf "$BACKUP_DIR"

echo ""
echo "Backup contents:"
tar tzf "$BACKUP_FILE" | head -20
echo "..."
echo ""
echo "Total files backed up: $(tar tzf "$BACKUP_FILE" | wc -l)"
