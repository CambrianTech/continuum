# Agent Scripts Architecture

## Overview

The agent-scripts directory contains a complete **"Deep Space Probe"** paradigm for browser automation and debugging. This system allows you to send JavaScript commands to browsers via WebSocket telemetry, mimicking how space missions communicate with remote probes.

## Directory Structure

```
agent-scripts/
├── tools/                    # Core agent tools
│   ├── python/              # Python-based tools
│   │   ├── js-send.py       # Main probe communication tool
│   │   ├── heal.py          # Universal healing system
│   │   ├── health-monitor.py # System health monitoring
│   │   ├── probe-safe.py    # Safe probe operations
│   │   └── setup.py         # Environment setup
│   └── javascript/          # JavaScript tools (future)
├── examples/                # Example scripts and demonstrations
│   ├── jokes/               # Humor examples
│   ├── diagnostics/         # System diagnostic examples
│   └── fixes/               # Browser/console fix examples
├── bin/                     # Executable wrappers
├── docs/                    # Documentation
└── requirements.txt         # Python dependencies
```

## Core Concepts

### Deep Space Probe Paradigm

The browser is treated as a **deep space probe** that can only be controlled through telemetry:

- **Base64 encoding** prevents syntax/escape issues during transmission
- **WebSocket telemetry** provides bidirectional communication
- **Auto-healing** automatically detects and fixes common issues
- **Console capture** returns execution results and logs

### Key Tools

#### js-send.py - Probe Communication
The main tool for sending JavaScript to browsers with auto-healing capabilities.

#### heal.py - Universal Healing
Pattern-based error detection and automatic recovery system.

#### Virtual Environment Management
Self-managing Python virtual environment in `.continuum/venv/agents/`.

## Usage Patterns

### Basic JavaScript Execution
```bash
# Direct JavaScript
js-send 'console.log("test")'

# From file
js-send script.js

# With options
js-send --quiet --json 'document.title = "New"'
```

### Auto-Healing
```bash
# Automatic healing based on error patterns
heal "Connection refused"

# Monitor and heal continuously
heal --monitor
```

### Development Workflow
1. Write JavaScript for browser execution
2. Send via js-send with base64 encoding
3. Monitor console telemetry response
4. Auto-healing handles connection issues
5. Iterate rapidly without manual intervention

## Benefits

- **No browser restart needed** - Live debugging via WebSocket
- **Cross-platform** - Works with any WebSocket-capable browser
- **Self-healing** - Automatically recovers from common issues
- **Clean telemetry** - Console spam prevention and smart throttling
- **Webpack-style hot reload** - Immediate feedback loop

This architecture enables rapid browser automation and debugging with minimal setup and maximum reliability.