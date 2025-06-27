# Connect Command - Standard Connection Protocol

## Overview
The Connect command implements the standard AI connection workflow for Continuum OS. Any AI can call this command to establish a proper development session.

## What It Does

### 1. Session Management
- **Auto-detect** existing browser sessions 
- **Connect** to existing tabs if available
- **Start DevTools session** if no browser connected

### 2. Browser Reload
- Triggers reload of all connected browsers
- Ensures latest code changes are active

### 3. Log Connection (JTAG Unit)
- Establishes real-time log streaming
- Enables stimulus-response debugging
- Console forwarding from browser to server

### 4. Session Sandbox
- Sets up isolated session environment
- WebSocket-based client separation

### 5. SelfTest Validation
- Runs system validation (like git hook)
- Ensures everything is working properly

## Usage

### From AI Portal
```bash
python3 ai-portal.py --cmd connect
python3 ai-portal.py --cmd connect --params '{"mode": "devtools"}'
```

### From JavaScript/Browser
```javascript
await continuum.connect();
await continuum.connect({mode: 'existing', selftest: false});
```

### From WebSocket API
```json
{
  "type": "connect",
  "params": {
    "mode": "auto",
    "reload": true,
    "selftest": true
  }
}
```

## Parameters

- **mode**: `'auto'` | `'existing'` | `'devtools'` - Connection mode
- **reload**: `boolean` - Trigger browser reload (default: true)
- **selftest**: `boolean` - Run validation (default: true) 
- **logs**: `boolean` - Connect to logs (default: true)
- **sandbox**: `boolean` - Setup isolation (default: true)

## Integration with Portal

When an AI runs the portal with `--devtools`, it automatically calls this connect command, establishing the full development environment without manual intervention.

This ensures consistent, reliable AI development sessions every time.