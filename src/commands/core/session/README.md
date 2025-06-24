# Session Command - Unified Session Management

**Low-level OS-like session management for all Continuum systems**

## Overview

The Session Command provides a unified session management system that organizes all Continuum activities into structured session directories with consistent artifacts and easy access patterns.

## Session Architecture

```
sessions/
├── verification/          # Git hook sessions
│   ├── run_f663cd79/
│   ├── run_76a23a1c/
│   ├── latest -> run_76a23a1c
│   └── history.txt
├── portal/               # AI portal sessions  
│   ├── run_screenshot_test/
│   ├── run_js_execution/
│   ├── latest -> run_js_execution
│   └── history.txt
├── personas/             # Persona execution sessions
│   ├── run_testing_droid_240623/
│   ├── latest -> run_testing_droid_240623
│   └── history.txt
├── sentinels/            # Sentinel monitoring sessions
│   ├── run_file_watcher_183045/
│   ├── latest -> run_file_watcher_183045
│   └── history.txt
└── devtools/             # DevTools automation sessions
    ├── run_browser_automation/
    ├── latest -> run_browser_automation
    └── history.txt
```

## Standard Artifacts

Each session directory contains:
- `session.json` - Session metadata and timing
- `client-logs.txt` - Client-side activity logs
- `server-logs.txt` - Server-side activity logs  
- `ui-capture.png` - Screenshot/visual capture
- Custom artifacts as needed

## Usage Examples

### Create Session
```bash
continuum session '{"action": "create", "type": "portal", "runId": "screenshot_test", "metadata": {"user": "claude"}}'
```

### Complete Session
```bash
continuum session '{"action": "complete", "type": "portal", "runId": "screenshot_test", "results": {"success": true, "summary": "Screenshot captured"}}'
```

### Read Artifacts
```bash
# Read latest portal client logs
continuum session '{"action": "read", "type": "portal", "runId": "latest", "artifact": "client-logs"}'

# Read specific verification screenshot
continuum session '{"action": "read", "type": "verification", "runId": "f663cd79", "artifact": "ui-capture"}'
```

### Write Artifacts
```bash
continuum session '{"action": "write", "type": "portal", "runId": "latest", "artifact": "debug-info", "content": "Debug message"}'
```

### List Sessions
```bash
continuum session '{"action": "list", "type": "verification", "limit": 5}'
```

### Get Paths
```bash
# Get latest portal session path
continuum session '{"action": "path", "type": "portal", "runId": "latest"}'

# Get specific session path
continuum session '{"action": "path", "type": "verification", "runId": "f663cd79"}'
```

## Integration with Other Systems

### Portal Integration
```javascript
// Portal creates session for screenshot
const sessionPath = await continuum.session({
  action: 'create',
  type: 'portal', 
  runId: `screenshot_${timestamp}`,
  metadata: { command: 'screenshot', filename: 'test.png' }
});

// Portal completes session with results
await continuum.session({
  action: 'complete',
  type: 'portal',
  runId: `screenshot_${timestamp}`,
  results: { 
    success: true, 
    summary: 'Screenshot captured',
    filepath: '/sessions/portal/run_screenshot_123/ui-capture.png'
  }
});
```

### Verification System Integration
```javascript
// Git hook creates verification session
const sessionPath = await continuum.session({
  action: 'create',
  type: 'verification',
  runId: commitHash,
  metadata: { commit: commitMessage, branch: currentBranch }
});

// Add artifacts during verification
await continuum.session({
  action: 'write',
  type: 'verification', 
  runId: commitHash,
  artifact: 'client-logs',
  content: clientLogData
});
```

## History Format

Unified history format across all session types:
```
MM/DD HH:mm STATUS DURATIONs RUN_ID SUMMARY
06/23 18:29 ✅   26.4s f663cd79 Screenshot captured successfully
06/23 18:32 ❌    5.2s abc12345 Browser connection failed
06/23 18:35 ✅   12.1s def67890 Portal JS execution completed
```

## Benefits

- **Human Readable**: Easy to find "what did X do last?" at `sessions/X/latest/`
- **Agent Coordination**: AIs can check other agents' work via session paths
- **Debugging**: Clear separation and artifact organization
- **Scalable**: Easy to add new session types
- **Consistent**: Same structure and interface across all systems

## Migration

Migrate existing verification data:
```bash
continuum session '{"action": "migrate", "legacyDir": "verification"}'
```

This preserves all existing verification sessions while organizing them in the new unified structure.