#!/bin/bash
# Migrate persona logs from .continuum/jtag/logs/{name}/ to .continuum/personas/{name}/logs/

set -e

echo "🔄 Migrating persona logs to persona directories..."

OLD_LOGS_DIR=".continuum/jtag/logs"
PERSONAS_DIR=".continuum/personas"

# Find all persona log directories (exclude system subdirs)
for persona_log_dir in "$OLD_LOGS_DIR"/*; do
  if [ -d "$persona_log_dir" ] && [ "$(basename "$persona_log_dir")" != "system" ]; then
    persona_name=$(basename "$persona_log_dir")
    
    # Find matching persona directory
    matching_persona=$(ls -d "$PERSONAS_DIR"/${persona_name}* 2>/dev/null | head -1)
    
    if [ -n "$matching_persona" ]; then
      target_logs_dir="$matching_persona/logs"
      
      echo "  📁 Migrating $persona_name logs..."
      mkdir -p "$target_logs_dir"
      
      # Move log files
      if [ -n "$(ls -A "$persona_log_dir" 2>/dev/null)" ]; then
        mv "$persona_log_dir"/*.log "$target_logs_dir/" 2>/dev/null || true
        echo "     ✓ Moved logs to $target_logs_dir"
      fi
      
      # Remove old directory
      rmdir "$persona_log_dir" 2>/dev/null || true
    else
      echo "  ⚠️  No persona directory found for $persona_name, skipping..."
    fi
  fi
done

echo "✅ Migration complete!"
echo ""
echo "Old location: $OLD_LOGS_DIR/{persona-name}/*.log"
echo "New location: $PERSONAS_DIR/{persona-name-id}/logs/*.log"
