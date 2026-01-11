#!/bin/bash
# Auto-cleanup observability data (keep last 1 hour only)
# Run this periodically to prevent DB bloat

# Use DATABASE_DIR env var or default to machine-level location
DATABASE_DIR="${DATABASE_DIR:-$HOME/.continuum/data}"
DB_PATH="$DATABASE_DIR/database.sqlite"

if [ ! -f "$DB_PATH" ]; then
  exit 0  # Silent exit if no DB
fi

# Delete observability data older than 1 hour
sqlite3 "$DB_PATH" <<SQL 2>/dev/null
DELETE FROM cognition_state_snapshots WHERE created_at < datetime('now', '-1 hour');
DELETE FROM cognition_plan_records WHERE created_at < datetime('now', '-1 hour');
DELETE FROM adapter_decision_logs WHERE created_at < datetime('now', '-1 hour');
DELETE FROM response_generation_logs WHERE created_at < datetime('now', '-1 hour');
DELETE FROM coordination_decisions WHERE created_at < datetime('now', '-1 hour');
SQL

# VACUUM quarterly (every 4th run) to reclaim space
if [ $((RANDOM % 4)) -eq 0 ]; then
  sqlite3 "$DB_PATH" "VACUUM;" 2>/dev/null
fi
