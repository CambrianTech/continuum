# Agent Scripts - Deep Space Probe Portal

This directory contains a complete **agent automation system** for browser control and debugging via the "Deep Space Probe" paradigm.

## Quick Start

```bash
# Send JavaScript to browser
js-send 'console.log("Hello from agent portal!")'

# Execute file
js-send examples/jokes/ai-joke.js

# Auto-heal issues
heal "Connection refused"
```

## Features

- 🛰️ **Deep Space Probe Paradigm** - Browser control via WebSocket telemetry
- 🔧 **Auto-Healing** - Automatic error detection and recovery
- 📡 **Base64 Encoding** - Safe JavaScript transmission
- 🔄 **Hot Reload** - Webpack-style development workflow
- 🧹 **Console Cleanup** - Smart spam prevention and throttling

## Directory Structure

```
agent-scripts/
├── tools/python/         # Core Python tools (js-send, heal, etc.)
├── examples/             # Example scripts by category
│   ├── jokes/           # Fun demonstration scripts
│   ├── diagnostics/     # System analysis tools
│   └── fixes/           # Browser/console fixes
├── bin/                 # Executable wrappers with auto-venv
├── docs/                # Architecture and usage documentation
└── requirements.txt     # Python dependencies
```

## Core Tools

- **js-send** - Main probe communication tool with auto-healing
- **heal** - Universal error detection and recovery system  
- **probe** - Safe probe operations wrapper
- **health-monitor** - Continuous system monitoring

## Deep Space Probe Paradigm

Think of the browser as a **deep space probe** that you control remotely. Your only communication is through WebSocket telemetry coming back to mission control (server logs). You must never break the communication link.

### Development Workflow

1. **Test fixes live** → Use `js-send` for immediate testing
2. **Once fix works** → Put it in actual client-side source code
3. **Restart server/client** → Test the permanent fix
4. **If broken** → Revert immediately and make smaller changes

### Safety Protocol

- Make tiny changes each time
- All JavaScript automatically base64 encoded for safe transmission
- Monitor server logs for telemetry
- If communication fails, auto-healing kicks in

## Usage Examples

### Basic Commands
```bash
# Direct JavaScript
js-send 'console.log("Hello probe!"); "status_ok"'

# From file
js-send examples/diagnostics/console-probe.js

# Quiet mode (minimal output)
js-send --quiet 'console.log("test")'

# JSON output only (for programmatic use)
js-send --json 'console.log("test")'

# Custom port
CONTINUUM_PORT=8080 js-send 'console.log("test")'
```

### Auto-Healing
```bash
# Automatic healing based on error patterns
heal "Connection refused"

# Monitor and heal continuously
heal --monitor

# Output:
# 🔧 HEALING: Server connection refused
# 📊 No server process found
# ✅ Auto-restart successful
```

### Reading Probe Telemetry

All browser console output comes back through WebSocket in server logs:

```
📱 Browser console output:
   [log] 🛰️ CLIENT: Executing probe telemetry command...
   [log] ✅ JavaScript sent to probe successfully
```

This is your window into the probe's status.

## Virtual Environment

The system automatically manages its Python virtual environment in `.continuum/venv/agents/`. No manual setup required - just run the tools and they'll self-configure.

### Environment Structure
- `.continuum/venv/agents/` - Shared environment for all agent scripts
- `.continuum/venv/web/` - Future: Web development tools  
- `.continuum/venv/testing/` - Future: Testing frameworks
- `.continuum/venv/deployment/` - Future: Deployment scripts

## Documentation

- [Architecture Overview](docs/ARCHITECTURE.md) - System design and concepts
- [Examples Guide](docs/EXAMPLES.md) - Usage examples and templates
- [Requirements](requirements.txt) - Python dependencies

## Emergency Protocols

If you break the probe:

1. **STOP** - Don't send more commands
2. **DIAGNOSE** - Run `heal --auto`
3. **RECOVER** - Auto-healing will attempt restart
4. **VERIFY** - Check with `js-send 'console.log("probe test")'`
5. **RESUME** - Only after verification passes

---

*This agent portal enables rapid browser automation with minimal setup and maximum reliability through the Deep Space Probe paradigm.*