#!/bin/bash
# Clean up stale .continuum data after backup
# Based on audit from docs/CONTINUUM-AUDIT-2025-11-28.md

set -e

echo "🧹 Cleaning up stale .continuum data..."
echo ""

# Check backup exists
BACKUP_FILE=$(ls -t backups/continuum-backup-2025-11-28-*.tgz 2>/dev/null | head -1)
if [ -z "$BACKUP_FILE" ]; then
  echo "❌ ERROR: No backup found! Run create-backup.sh first."
  exit 1
fi

echo "✅ Backup found: $BACKUP_FILE ($(du -h "$BACKUP_FILE" | cut -f1))"
echo ""

# 1. Remove OLD UUID-based logs (superseded)
if [ -d ".continuum/logs/personas" ]; then
  echo "🗑️  Removing old UUID-based persona logs..."
  SIZE=$(du -sh .continuum/logs/personas 2>/dev/null | cut -f1 || echo "0")
  echo "   Size: $SIZE"
  rm -rf .continuum/logs/personas
  echo "   ✅ Removed"
fi

# 2. Clean old validation runs (>7 days)
echo ""
echo "🗑️  Cleaning validation runs older than 7 days..."
if [ -d ".continuum/sessions/validation" ]; then
  BEFORE=$(du -sh .continuum/sessions/validation 2>/dev/null | cut -f1 || echo "0")
  echo "   Before: $BEFORE"
  find .continuum/sessions/validation -type d -mindepth 1 -maxdepth 1 -mtime +7 -exec rm -rf {} + 2>/dev/null || true
  AFTER=$(du -sh .continuum/sessions/validation 2>/dev/null | cut -f1 || echo "0")
  echo "   After: $AFTER"
  echo "   ✅ Cleaned"
fi

# 3. Clean old backups (>7 days)
echo ""
echo "🗑️  Cleaning database backups older than 7 days..."
if [ -d ".continuum/jtag/backups" ]; then
  BEFORE=$(du -sh .continuum/jtag/backups 2>/dev/null | cut -f1 || echo "0")
  echo "   Before: $BEFORE"
  find .continuum/jtag/backups -type f -mtime +7 -delete 2>/dev/null || true
  AFTER=$(du -sh .continuum/jtag/backups 2>/dev/null | cut -f1 || echo "0")
  echo "   After: $AFTER"
  echo "   ✅ Cleaned"
fi

# 4. Clean temp files (>7 days)
echo ""
echo "🗑️  Cleaning temp files older than 7 days..."
if [ -d ".continuum/media/temp" ]; then
  BEFORE=$(du -sh .continuum/media/temp 2>/dev/null | cut -f1 || echo "0")
  echo "   Before: $BEFORE"
  find .continuum/media/temp -type f -mtime +7 -delete 2>/dev/null || true
  AFTER=$(du -sh .continuum/media/temp 2>/dev/null | cut -f1 || echo "0")
  echo "   After: $AFTER"
  echo "   ✅ Cleaned"
fi

# 5. Clean stale performance data (>14 days)
echo ""
echo "🗑️  Cleaning performance data older than 14 days..."
if [ -d ".continuum/jtag/performance" ]; then
  BEFORE=$(du -sh .continuum/jtag/performance 2>/dev/null | cut -f1 || echo "0")
  echo "   Before: $BEFORE"
  find .continuum/jtag/performance -type f -mtime +14 -delete 2>/dev/null || true
  AFTER=$(du -sh .continuum/jtag/performance 2>/dev/null | cut -f1 || echo "0")
  echo "   After: $AFTER"
  echo "   ✅ Cleaned"
fi

echo ""
echo "✅ Cleanup complete!"
echo ""

# Show final sizes
echo "📊 Current .continuum size:"
du -sh .continuum 2>/dev/null || echo "   Error calculating size"

echo ""
echo "📂 Current structure:"
echo "   Active database: $(ls -lh .continuum/jtag/data/database.sqlite 2>/dev/null | awk '{print $5, $6, $7, $8}')"
echo "   New logs: $(find .continuum/jtag/logs -name '*.log' 2>/dev/null | wc -l | xargs) log files"
echo "   Backup: $BACKUP_FILE ($(du -h "$BACKUP_FILE" | cut -f1))"
