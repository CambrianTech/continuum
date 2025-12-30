#!/bin/bash
# MCP wrapper script - ensures correct working directory for imports
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"
exec npx tsx mcp-server.ts
