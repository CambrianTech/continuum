#!/bin/bash
# Selective .continuum backup before cleanup
# Excludes stale test data to save time

set -e

BACKUP_DATE=$(date +%Y-%m-%d-%H%M%S)
BACKUP_FILE="/Volumes/FlashGordon/cambrian/continuum/backups/continuum-backup-${BACKUP_DATE}.tgz"
CONTINUUM_DIR=".continuum"

echo "🗜️  Creating selective .continuum backup..."
echo "📦 Output: ${BACKUP_FILE}"
echo ""

# Create temp exclude file
EXCLUDE_FILE=$(mktemp)
cat > "$EXCLUDE_FILE" <<'EOF'
# EXCLUDE: Stale test data (1.15GB)
sessions/validation

# EXCLUDE: Old backups (>7 days old, redundant)
jtag/backups

# EXCLUDE: Stale performance metrics (19 days old)
jtag/performance

# EXCLUDE: Temp training files (13 days old)
media/temp

# EXCLUDE: Old UUID-based logs (superseded by name-based logs)
logs/personas
EOF

echo "🚫 Excluding:"
cat "$EXCLUDE_FILE" | grep -v "^#" | sed 's/^/   - /'
echo ""

echo "✅ Including:"
echo "   - jtag/data/database.sqlite (ACTIVE DATABASE)"
echo "   - jtag/logs/{name}/ (NEW NAME-BASED LOGS)"
echo "   - jtag/registry/ (PROCESS TRACKING)"
echo "   - datasets/ (TRAINING DATA REFERENCE)"
echo "   - genome/ (PYTHON ENVIRONMENT)"
echo ""

# Create backup with exclusions
tar -czf "$BACKUP_FILE" \
  --exclude-from="$EXCLUDE_FILE" \
  -C "." \
  "$CONTINUUM_DIR" \
  2>&1 | grep -v "tar: Removing leading"

# Clean up
rm "$EXCLUDE_FILE"

# Show results
BACKUP_SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
echo ""
echo "✅ Backup complete!"
echo "📊 Size: ${BACKUP_SIZE}"
echo "📁 Location: ${BACKUP_FILE}"
echo ""

# Verify contents
echo "📋 Backup contents:"
tar -tzf "$BACKUP_FILE" | head -20
echo "   ... (use 'tar -tzf ${BACKUP_FILE}' to see all files)"
echo ""

echo "🔍 Verification:"
if tar -tzf "$BACKUP_FILE" | grep -q "jtag/data/database.sqlite"; then
  echo "   ✅ Active database included"
else
  echo "   ❌ WARNING: Active database NOT found in backup!"
fi

if tar -tzf "$BACKUP_FILE" | grep -q "jtag/logs/"; then
  echo "   ✅ Log directories included"
else
  echo "   ❌ WARNING: Log directories NOT found in backup!"
fi

echo ""
echo "✅ Safe to proceed with cleanup!"
