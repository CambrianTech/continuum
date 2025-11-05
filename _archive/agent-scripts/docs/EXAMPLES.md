# Agent Scripts Examples

## Quick Start Examples

### Basic JavaScript Execution
```bash
# Simple console logging
js-send 'console.log("Hello from the probe!")'

# DOM manipulation
js-send 'document.title = "Controlled by Agent Portal"'

# Multiple commands
js-send 'console.log("Step 1"); alert("Step 2"); console.log("Complete");'
```

### File-based Execution
```bash
# Execute JavaScript from file
js-send examples/jokes/ai-joke.js

# Diagnostic scripts
js-send examples/diagnostics/console-probe.js
```

## Example Categories

### 🎭 Jokes (`examples/jokes/`)
Demonstration scripts that show basic alert functionality:
- `ai-joke.js` - AI therapy humor
- `css-joke.js` - CSS relationship problems
- `tooth-joke.js` - Dental root directory pun

### 🔍 Diagnostics (`examples/diagnostics/`)
Scripts for system analysis and debugging:
- `console-probe.js` - Inspect current console state
- `error-capture.js` - Capture and analyze errors/warnings
- `probe-test.js` - Basic connectivity testing

### 🔧 Fixes (`examples/fixes/`)
Comprehensive browser issue resolution:
- `comprehensive-fix.js` - Multi-layered console spam prevention
- `websocket-fix.js` - WebSocket connection stabilization

## Auto-Healing Examples

### Connection Recovery
```bash
# Auto-detect and fix connection issues
heal "Connection refused"

# Output:
# 🔧 HEALING: Server connection refused
# 📊 No server process found
# ✅ Auto-restart successful
```

### Monitor Mode
```bash
# Continuous healing monitoring
heal --monitor

# Runs diagnostics every 30 seconds and auto-fixes issues
```

## Advanced Usage

### Quiet Mode
```bash
# Minimal output for scripting
js-send --quiet 'console.log("silent execution")'
# Output: ✅ EXECUTED
```

### JSON Output
```bash
# Machine-readable results
js-send --json 'document.title'
# Returns full JSON response with telemetry
```

### Error Handling
```bash
# The system gracefully handles:
# - Server restarts
# - WebSocket disconnections  
# - Browser crashes
# - Network issues
# - Syntax errors in JavaScript
```

## Development Tips

1. **Use Base64 by default** - All JavaScript is automatically base64 encoded
2. **Monitor console telemetry** - Check browser console for execution feedback
3. **Leverage auto-healing** - Let the system fix common issues automatically
4. **Test incrementally** - Send small scripts first, then build complexity
5. **Use examples as templates** - Copy and modify existing examples

## Creating New Examples

### Template Structure
```javascript
// examples/new-category/my-script.js
console.log("🎯 Starting my custom script...");

// Your JavaScript logic here
const result = document.querySelector('#my-element');
console.log("Found element:", result);

// Always end with a status
console.log("✅ Script completed successfully");
```

### Adding to Categories
- Create new directories under `examples/` for new categories
- Follow naming convention: lowercase with hyphens
- Include descriptive comments in scripts
- Test with `js-send examples/new-category/my-script.js`

This examples system provides a foundation for rapid browser automation and serves as documentation for the agent portal capabilities.