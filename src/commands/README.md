# Commands Directory - Drop-in Command System

This directory contains **dynamically discoverable commands** for the Continuum system. Just like Unix commands in `/bin/`, any command dropped here automatically becomes available everywhere.

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

## 📁 Directory Structure

```
src/commands/
├── README.md                    # This file
├── core/                        # Core system commands
│   ├── BrowserJSCommand.cjs     # Execute JavaScript in browser
│   ├── ExecCommand.cjs          # Execute shell commands
│   ├── ReloadCommand.cjs        # Reload browser tabs
│   └── ScreenshotCommand.cjs    # Take screenshots
├── browser/                     # Browser-specific commands
├── automation/                  # Automation commands
└── gaming/                      # Gaming-related commands
```

## 🎯 Example: Simple Command

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

## 🚀 Just Drop and Go!

That's it! Drop your command file in this directory following the interface contract, and it immediately becomes part of the Continuum command ecosystem. No restarts, no configuration, no manual registration needed.

**The system discovers and documents itself automatically.**