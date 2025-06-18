# Commands Directory - Self-Contained Command Packages

This directory contains **self-contained command packages** for the Continuum system. Each command is a fully independent package that defines its complete behavioral contract including timeouts, retries, concurrency rules, and dual-side execution patterns.

## 🚀 How It Works

1. **Drop command file** into any subdirectory here
2. **Implements the standard interface** (see below)
3. **Automatically discovered** by CommandProcessor
4. **Shows up everywhere instantly:**
   - WebSocket connection banner
   - `continuum --help` output
   - `/connect` API endpoint documentation
   - Agent portal examples
   - Dynamic usage guides

**No configuration files, no registration, no hardcoded lists needed!**

## 📋 Command Interface Contract

Every command must implement this interface:

```javascript
class YourCommand {
  static getDefinition() {
    return {
      name: 'COMMAND_NAME',           // Uppercase command identifier
      description: 'What it does',    // Brief description
      params: '<param_format>',       // Parameter format/syntax
      examples: [                     // Usage examples (optional)
        'example_param_1',
        'example_param_2'
      ],
      category: 'Core',              // Core|Gaming|Browser|Custom
      icon: '🎯'                     // Emoji icon (optional)
    };
  }
  
  static async execute(params, continuum, encoding = 'utf-8') {
    // Your command implementation here
    
    return {
      executed: true,                // Required: boolean success
      message: 'Success message',    // Optional: human readable result
      result: 'return_value',        // Optional: actual result data
      error: null                    // Optional: error message if failed
    };
  }
}

module.exports = YourCommand;
```

## 🔧 Parameters

- **`params`** - The parameter string passed to your command
- **`continuum`** - Full access to Continuum instance (WebSocket, browser control, etc.)
- **`encoding`** - Parameter encoding (usually 'utf-8' or 'base64')

## 📁 Package Structure

Each command is a **complete package** with dual-side execution capabilities:

```
src/commands/core/[command]/
├── [Command]Command.cjs         # Server-side implementation
├── [Command]Command.client.js   # Client-side implementation (optional)
├── index.server.js              # Module definition and registration
├── package.json                 # 🎯 Package rules and execution contract
└── test/                        # Command-specific tests
```

### Example Command Packages
```
src/commands/
├── README.md                    # This file
├── core/                        # Core system commands
│   ├── restart/                 # Server restart package
│   │   ├── RestartCommand.cjs
│   │   ├── index.server.js
│   │   └── package.json         # timeout: 70s client, 30s server
│   ├── screenshot/              # Screenshot capture package  
│   │   ├── ScreenshotCommand.cjs
│   │   ├── ScreenshotCommand.client.js
│   │   ├── index.server.js
│   │   └── package.json         # timeout: 30s client, 15s server
│   ├── workspace/               # Workspace management package
│   │   ├── WorkspaceCommand.cjs
│   │   ├── index.server.js
│   │   └── package.json         # timeout: 5s, concurrent: true
│   └── sentinel/                # AI monitoring package
│       ├── SentinelCommand.cjs
│       ├── index.server.js
│       └── package.json         # timeout: 45s, persistent: true
├── browser/                     # Browser-specific packages
├── automation/                  # Automation packages
└── gaming/                      # Gaming-related packages
```

## 🎯 Package-Defined Execution Rules

Each command package defines its **complete execution contract** in `package.json`:

### Example: Restart Command Package
```javascript
// src/commands/core/restart/package.json
{
  "name": "@continuum/restart-command",
  "version": "1.2.0",
  "description": "Server restart with version management",
  "timeouts": {
    "client": 70.0,        // How long client should wait
    "server": 30.0         // How long server execution should take
  },
  "retries": {
    "client": 1,           // Client retry attempts
    "server": 0            // Server doesn't retry restart
  },
  "behavior": {
    "client": "wait_and_auto_heal",
    "server": "kill_self_after_response"
  },
  "concurrency": {
    "client": false,       // Don't allow multiple restart calls
    "server": false        // Server can't handle concurrent restarts
  },
  "sideEffects": ["version_bump", "process_restart", "file_system"]
}
```

### Example: Screenshot Command Package
```javascript
// src/commands/core/screenshot/package.json
{
  "name": "@continuum/screenshot-command",
  "version": "2.1.0",
  "description": "Desktop screenshot capture with browser integration",
  "timeouts": {
    "client": 30.0,        // Client waits for image capture + processing
    "server": 15.0         // Server execution: capture + save + respond
  },
  "retries": {
    "client": 2,           // Client retries on network issues
    "server": 1            // Server retries on capture failures
  },
  "resources": {
    "client": ["display_access", "file_system"],
    "server": ["screenshot_api", "file_storage", "browser_connection"]
  },
  "concurrency": {
    "client": true,        // Multiple screenshot requests OK
    "server": true         // Server can handle concurrent captures
  },
  "sideEffects": ["creates_files", "system_capture"]
}
```

## 🔄 Dual-Side Execution Model

Commands can execute on **both client and server** with different requirements:

### Server-Side Implementation
```javascript
// RestartCommand.cjs
class RestartCommand extends BaseCommand {
  static async execute(params, continuum) {
    const rules = require('./package.json');
    const serverTimeout = rules.timeouts.server * 1000;
    
    // Server enforces its own execution timeout
    return await Promise.race([
      this.actualRestart(params),
      this.timeoutAfter(serverTimeout)
    ]);
  }
}
```

### Client-Side Implementation  
```javascript
// ScreenshotCommand.client.js (for browser-specific logic)
export class ScreenshotClientCommand {
  static async execute(params) {
    const rules = await import('./package.json');
    const clientTimeout = rules.timeouts.client * 1000;
    
    return await Promise.race([
      this.captureDOMAndSend(params),
      this.clientTimeout(clientTimeout)
    ]);
  }
}
```

## 🤖 AI Portal Integration

The AI Portal respects each command's package rules:

```python
# python-client/ai-portal.py
async def run_command(cmd: str, params: str = "{}"):
    # Get command-specific rules from package.json
    rules = await get_command_package_rules(cmd)
    
    client_timeout = rules.get('timeouts', {}).get('client', 10.0)
    retries = rules.get('retries', {}).get('client', 2)
    auto_heal = rules.get('behavior', {}).get('client') == 'wait_and_auto_heal'
    
    # Apply command's rules
    for attempt in range(retries):
        try:
            async with asyncio.timeout(client_timeout):
                result = await client.send_command(cmd, params)
                return result
        except asyncio.TimeoutError:
            if auto_heal and cmd == 'restart':
                return handle_restart_timeout()
```

## 🎯 Example: Simple Command Package

```javascript
// src/commands/core/HelloCommand.cjs
class HelloCommand {
  static getDefinition() {
    return {
      name: 'HELLO',
      description: 'Send greeting message',
      params: '<message>',
      examples: [
        'Hello World!',
        'Greetings from Continuum'
      ],
      category: 'Core',
      icon: '👋'
    };
  }
  
  static async execute(params, continuum) {
    // Send message to browser console
    if (continuum.webSocketServer) {
      continuum.webSocketServer.broadcast({
        type: 'execute_js',
        data: {
          command: `console.log('Hello: ${params}');`,
          timestamp: new Date().toISOString()
        }
      });
    }
    
    return {
      executed: true,
      message: `Greeting sent: ${params}`,
      result: params
    };
  }
}

module.exports = HelloCommand;
```

## 🛰️ Usage Examples

Once your command is dropped in, it's immediately available:

### WebSocket
```json
{"type": "task", "role": "system", "task": "[CMD:HELLO] Hello from WebSocket!"}
```

### HTTP API
```bash
curl -X POST http://localhost:9000/connect \
  -H "Content-Type: application/json" \
  -d '{"command": "HELLO", "params": "Hello from API!"}'
```

### Agent Scripts
```bash
# If you have a wrapper in agent-scripts
hello-send "Hello from agent portal!"
```

## 🔄 Auto-Discovery Process

1. **CommandProcessor scans** this directory recursively
2. **Loads all `.cjs` files** that export command classes
3. **Validates interface** (has `getDefinition()` and `execute()`)
4. **Registers commands** in the global command registry
5. **Updates documentation** automatically everywhere

## 📚 Command Categories

- **Core** - Essential system commands (EXEC, BROWSER_JS, etc.)
- **Browser** - Browser automation and control
- **Gaming** - Game-specific automation
- **Automation** - General automation tasks
- **Custom** - User-defined commands

## 🔧 Advanced Features

### Access Continuum Instance
```javascript
// In your execute() method
continuum.webSocketServer.broadcast(message);    // Send to browsers
continuum.commandProcessor.execute(otherCmd);    // Call other commands
continuum.activeConnections.size;               // Get connection count
```

### Error Handling
```javascript
static async execute(params, continuum) {
  try {
    // Your logic here
    return { executed: true, result: 'success' };
  } catch (error) {
    return { 
      executed: false, 
      error: error.message,
      stack: error.stack 
    };
  }
}
```

### Parameter Validation
```javascript
static async execute(params, continuum) {
  if (!params || params.trim() === '') {
    return {
      executed: false,
      error: 'Parameter required',
      usage: 'HELLO <message>'
    };
  }
  
  // Continue with execution...
}
```

## 🌟 Key Architecture Principles

### 1. Self-Contained Packages
Each command is a **complete executable package** that defines its own:
- **Execution timeouts** (client vs server requirements)
- **Retry strategies** (network vs logic failures)
- **Concurrency rules** (single-threaded vs parallel-safe)
- **Resource requirements** (file system, display, browser, etc.)
- **Side effects** (creates files, restarts processes, modifies state)

### 2. Dual-Side Execution Model
Commands execute on **both client and server** with different contracts:

```javascript
// Server: Handles actual work, enforces server timeout
static async execute(params, continuum) {
  const rules = require('./package.json');
  const timeout = rules.timeouts.server * 1000;
  return await Promise.race([
    this.actualWork(params),
    this.timeoutAfter(timeout)
  ]);
}
```

```python
# Client: Manages network, enforces client timeout + auto-healing
async def run_command(cmd, params):
  rules = await get_package_rules(cmd)
  timeout = rules.timeouts.client
  retries = rules.retries.client
  
  async with asyncio.timeout(timeout):
    return await client.send_command(cmd, params)
```

### 3. Command-Specific Behaviors
Different commands have different execution patterns:

- **RESTART**: Client waits 70s, server kills itself after 30s
- **SCREENSHOT**: Client waits 30s, server captures in 15s  
- **WORKSPACE**: Client waits 5s, server responds instantly
- **SENTINEL**: Client waits 45s, server runs persistent monitoring

### 4. Auto-Healing Integration
The AI Portal respects each command's behavior requirements:

```python
# For restart command: Expected timeout triggers auto-healing
if cmd == 'restart' and 'timeout' in error:
    return await handle_expected_restart_behavior()

# For other commands: Timeout indicates real failure
else:
    return await retry_with_auto_heal()
```

## 🎯 Package.json Contract Examples

### High-Performance Command (Workspace)
```javascript
{
  "timeouts": {"client": 5.0, "server": 1.0},
  "retries": {"client": 0, "server": 0},
  "concurrency": {"client": true, "server": true},
  "sideEffects": ["creates_directories"]
}
```

### Critical System Command (Restart)  
```javascript
{
  "timeouts": {"client": 70.0, "server": 30.0},
  "retries": {"client": 1, "server": 0},
  "behavior": {"client": "wait_and_auto_heal", "server": "kill_self"},
  "concurrency": {"client": false, "server": false},
  "sideEffects": ["version_bump", "process_restart", "file_system"]
}
```

### Monitoring Command (Sentinel)
```javascript
{
  "timeouts": {"client": 45.0, "server": 60.0},
  "retries": {"client": 2, "server": 1},
  "behavior": {"client": "persistent_connection", "server": "background_task"},
  "resources": {"server": ["log_files", "process_monitoring", "file_system"]},
  "concurrency": {"client": false, "server": true}
}
```

## 🚀 Just Drop and Go!

That's it! Drop your command file in this directory following the interface contract, and it immediately becomes part of the Continuum command ecosystem. No restarts, no configuration, no manual registration needed.

**The system discovers and documents itself automatically.**

---

## 🏗️ Architectural Brilliance Summary

This package-based architecture represents a breakthrough in modular system design:

1. **📦 Package-Defined Rules**: Each command package defines its complete execution contract
2. **🔄 Dual-Side Timeouts**: Client and server enforce their own appropriate timeouts  
3. **🤖 Intelligent Auto-Healing**: Clients respect command-specific failure behaviors
4. **🎯 Self-Documenting**: Help system generates live docs from package definitions
5. **🚀 Zero Configuration**: Drop files and go - no registration or config needed
6. **⚡ Command-Specific Optimization**: Each command optimized for its specific use case

This eliminates god objects, hardcoded timeouts, and brittle client-server coupling while enabling intelligent auto-healing and self-documentation.