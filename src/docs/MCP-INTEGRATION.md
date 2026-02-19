# JTAG MCP Server Integration

JTAG exposes all its commands as MCP (Model Context Protocol) tools, enabling any MCP-compatible AI client (Claude Code, Claude Desktop, VS Code extensions, etc.) to use JTAG commands directly.

## Quick Start (Claude Code CLI)

**One-time setup:**
```bash
cd src
npm run mcp:setup
```

This registers the JTAG MCP server with your Claude Code installation. All 158+ JTAG commands become available as tools.

**Verify it's working:**
```bash
claude mcp list
# Should show: jtag: ... - ✓ Connected
```

## Manual Setup (Alternative)

If the automated setup doesn't work:

```bash
cd src

# Make wrapper executable
chmod +x mcp-wrapper.sh

# Register with Claude Code
claude mcp add jtag -s user -- /full/path/to/jtag/mcp-wrapper.sh
```

## Claude Desktop Configuration

Add this to your Claude Desktop config (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "jtag": {
      "command": "/full/path/to/jtag/mcp-wrapper.sh"
    }
  }
}
```

Replace `/full/path/to/jtag` with the actual absolute path to the JTAG directory.

## How It Works

The MCP server dynamically generates tools from `generated-command-schemas.json`:

- **One-to-one mapping**: Each JTAG command becomes one MCP tool
- **Dynamic discovery**: No hardcoded command list - new commands appear automatically
- **Type-safe**: Parameters are validated against command schemas
- **Shared connection**: Reuses a single WebSocket connection for efficiency

## Tool Naming

JTAG command paths are converted to MCP tool names by replacing `/` with `_`:

| JTAG Command | MCP Tool |
|--------------|----------|
| `ping` | `ping` |
| `interface/screenshot` | `interface_screenshot` |
| `collaboration/chat/send` | `collaboration_chat_send` |

## Example Usage

Once configured, Claude can use JTAG tools directly:

```
User: Take a screenshot of the current page
Claude: [Uses interface_screenshot tool]
```

```
User: Send a message to the general chat
Claude: [Uses collaboration_chat_send tool with room="general"]
```

## Available Tools

Run `./jtag help --list` to see all available commands/tools.

Key tool categories:
- `interface_*` - Browser automation (screenshot, click, type, navigate)
- `collaboration_*` - Chat, decisions, activities
- `data_*` - Database operations
- `ai_*` - AI model operations, RAG, embeddings
- `logs_*` - Log viewing and searching
- `user_*` - User management

## Architecture

```
Claude Desktop / VS Code
       ↓ (MCP Protocol via stdio)
   mcp-server.ts
       ↓ (WebSocket)
   JTAG Server (port 9001)
       ↓
   Command Handlers
       ↓
   Browser / Database / AI
```

## Troubleshooting

### "Failed to connect" in Claude Code
Use the wrapper script instead of `npx tsx` directly:
```bash
# Remove broken config
claude mcp remove jtag -s user

# Re-add with wrapper script
claude mcp add jtag -s user -- /full/path/to/jtag/mcp-wrapper.sh
```

The wrapper script handles the working directory correctly, which is required for imports to resolve.

### "JTAG system not running" when calling tools
The MCP server can't connect to JTAG. Start the system:
```bash
cd src
npm start  # Wait ~90 seconds
```

Or use the `jtag_system_start` MCP tool - Claude can call this to start the system.

### Tools not appearing
Regenerate command schemas:
```bash
npx tsx generator/generate-command-schemas.ts
npm run mcp
```

### Connection timeout
Check that port 9001 is available and the WebSocket server is running:
```bash
./jtag ping --verbose
```

### Deadlock after `/mcp reconnect`
This is a [known Claude Code bug](https://github.com/anthropics/claude-code/issues/11385). If Claude Code freezes:
- Press Ctrl+Z to suspend
- Continue with `claude -c` to restore session

## Features

### Inline Image Results

Screenshots and other image results are returned **inline** as base64-encoded content. No separate file read is needed:

```
mcp__jtag__interface_screenshot(resultType="file")
→ Returns: { type: "image", data: "<base64>", mimeType: "image/jpeg" }
```

Images are automatically:
- Resized to max 1200x800 pixels
- Compressed to JPEG at 70% quality
- Reduced from ~800KB to ~70KB for efficient transport

### Tool Discovery

With 157+ tools, use `jtag_search_tools` to find relevant commands:

```
mcp__jtag__jtag_search_tools(query="widget")
→ Returns widget-css, widget-interact, widget-state, widget-events

mcp__jtag__jtag_search_tools(query="css", category="development")
→ Returns development/debug/widget-css
```

### Tool Categories

Tools are sorted by priority, with common ones first:
- **Priority 0**: ping, help, list
- **Priority 1**: screenshot, navigate, chat/send, chat/export
- **Priority 2**: ai/generate, ai/status
- **Priority 3+**: Grouped by category (interface/, collaboration/, ai/, data/, etc.)
