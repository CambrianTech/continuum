#!/bin/bash
# Setup JTAG MCP server for Claude Code
# Run this once to enable JTAG tools in Claude Code

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
JTAG_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
WRAPPER="$JTAG_DIR/mcp-wrapper.sh"

echo "Setting up JTAG MCP server..."
echo "JTAG directory: $JTAG_DIR"
echo "Wrapper script: $WRAPPER"

# Check if claude CLI is available (try common paths)
CLAUDE_CMD=""
if command -v claude &> /dev/null; then
    CLAUDE_CMD="claude"
elif [ -x "$HOME/.claude/local/claude" ]; then
    CLAUDE_CMD="$HOME/.claude/local/claude"
elif command -v npx &> /dev/null && npx --yes @anthropic-ai/claude-code --help &> /dev/null 2>&1; then
    CLAUDE_CMD="npx --yes @anthropic-ai/claude-code"
fi

if [ -z "$CLAUDE_CMD" ]; then
    echo ""
    echo "Claude Code CLI not found in PATH."
    echo ""
    echo "Manual setup - add to your Claude config (~/.claude.json):"
    echo ""
    echo '  "mcpServers": {'
    echo '    "jtag": {'
    echo '      "type": "stdio",'
    echo "      \"command\": \"$WRAPPER\""
    echo '    }'
    echo '  }'
    echo ""
    exit 0
fi

# Ensure wrapper is executable
chmod +x "$WRAPPER"

# Remove existing jtag MCP if present (ignore errors)
$CLAUDE_CMD mcp remove jtag -s user 2>/dev/null || true

# Add JTAG MCP server
$CLAUDE_CMD mcp add jtag -s user -- "$WRAPPER"

echo ""
echo "Verifying connection..."
$CLAUDE_CMD mcp list | grep -A1 jtag

echo ""
echo "JTAG MCP server configured successfully!"
echo ""
echo "Available tools (158+):"
echo "  - ping, interface_screenshot, interface_click, interface_navigate"
echo "  - collaboration_chat_send, collaboration_chat_export"
echo "  - data_list, data_read, data_create, data_update"
echo "  - ai_generate, ai_report, logs_search"
echo "  - And 150+ more..."
echo ""
echo "Usage: Just ask Claude to use JTAG tools like 'take a screenshot' or 'send a chat message'"
