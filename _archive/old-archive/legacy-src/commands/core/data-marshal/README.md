# Data Marshal Command

Universal data marshalling for cross-environment command chaining and autonomous development workflows.

## Overview

The `data-marshal` command enables seamless data flow between commands, environments, and protocols. Perfect for autonomous AI development where commands need to pass data reliably across different execution contexts.

## Core Features

- **Base64 encoding** for binary data (screenshots, files)
- **JSON serialization** for structured data
- **UUID correlation** for tracking across command chains  
- **Promise-based chaining** for complex workflows
- **WebSocket-safe transmission** with integrity checking
- **Path extraction** for accessing nested data

## Usage Examples

### Screenshot + Analysis Chain

```bash
# 1. Capture screenshot and marshal for transmission
./continuum screenshot --selector="body" | 
./continuum data-marshal --operation=encode --encoding=base64 --source=screenshot --destination=analysis

# 2. Chain JS execution to analyze screenshot content
./continuum js-execute --script="document.querySelectorAll('img').length" |
./continuum data-marshal --operation=chain --source=js-execute --destination=widget-inspect

# 3. Extract specific widget data
./continuum widget-inspect --selector=".sidebar" |
./continuum data-marshal --operation=extract --metadata='{"path":"widgets[0].tagName"}'
```

### JTAG Debugging Workflow

```bash
# Complete autonomous debugging chain
./continuum data-marshal --operation=chain --correlationId="debug-session-123" |\
  jtag.run('widget-inspect') |\
  jtag.run('screenshot', { selector: 'continuum-sidebar' }) |\
  jtag.run('js-execute', { script: 'window.getComputedStyle(document.querySelector("continuum-sidebar"))' })
```

### Live Widget Validation

```typescript
// Autonomous AI can chain commands for validation
const result = await jtag.run('data-marshal', {
  operation: 'chain',
  data: await jtag.inspectWidgets(),
  correlationId: 'widget-validation-' + Date.now()
});

// Extract specific widget health
const sidebarHealth = await jtag.run('data-marshal', {
  operation: 'extract', 
  data: result.data.chainable.data,
  metadata: { path: 'widgets.find(w => w.tagName === "continuum-sidebar")' }
});
```

## Operations

### `encode`
Encode data for cross-environment transmission:

```bash
# Encode screenshot data
./continuum data-marshal --operation=encode --data="<binary>" --encoding=base64

# Encode widget inspection results
./continuum data-marshal --operation=encode --data='{"widgets":[...]}' --encoding=json
```

### `decode` 
Decode previously marshalled data:

```bash
# Decode base64 screenshot
./continuum data-marshal --operation=decode --data='{"id":"...","data":"base64..."}'

# Decode JSON results
./continuum data-marshal --operation=decode --data='{"encoding":"json","data":"{...}"}'
```

### `chain`
Create chainable data for command composition:

```bash
# Create chainable widget data
./continuum data-marshal --operation=chain --data='{"widgets":[...]}' --destination=screenshot

# Chain with correlation ID for tracking
./continuum data-marshal --operation=chain --correlationId="debug-123" --source=js-execute
```

### `extract`
Extract specific fields from complex data:

```bash
# Extract widget tag name
./continuum data-marshal --operation=extract --metadata='{"path":"widgets[0].tagName"}'

# Extract multiple widget properties
./continuum data-marshal --operation=extract --metadata='{"path":"widgets.map(w => w.isConnected)"}'
```

## Output Format

### Encoded Data
```json
{
  "success": true,
  "data": {
    "marshalled": {
      "id": "marshal-1752172000-abc123",
      "timestamp": "2025-07-10T18:46:40.000Z", 
      "encoding": "base64",
      "originalType": "buffer",
      "size": 15420,
      "data": "iVBORw0KGgoAAAANSUhEUgAA...",
      "source": "screenshot",
      "destination": "analysis",
      "checksum": "a1b2c3d4"
    },
    "ready": true
  },
  "message": "Data marshalled successfully [marshal-1752172000-abc123] (15420 bytes, base64)"
}
```

### Chainable Data
```json
{
  "success": true,
  "data": {
    "chainable": {
      "marshalId": "chain-1752172000-def456",
      "ready": true,
      "data": { "widgets": [...] },
      "next": "[Function]",
      "extract": "[Function]"
    },
    "chainId": "chain-1752172000-def456"
  },
  "message": "Chainable data created [chain-1752172000-def456]"
}
```

## Integration with JTAG

The data marshal command perfectly complements JTAG debugging:

```typescript
// JTAG CLI automatically uses data marshalling for complex workflows
const debugChain = await jtag.run('data-marshal', {
  operation: 'chain',
  data: {
    screenshot: await jtag.screenshot('body'),
    widgets: await jtag.inspectWidgets(),
    jsResult: await jtag.run('js-execute', { 
      script: 'JSON.stringify(window.performance.timing)' 
    })
  },
  correlationId: 'autonomous-debug-session'
});

// AI can extract specific insights
const performanceMetrics = debugChain.extract('jsResult.data.result');
const widgetCount = debugChain.extract('widgets.totalWidgets');
```

## Benefits for Autonomous Development

1. **Reliable Data Flow**: UUID correlation tracks data across command chains
2. **Cross-Environment Safety**: Base64 encoding handles binary data over WebSockets  
3. **Integrity Checking**: Checksums prevent data corruption
4. **Promise Chaining**: Enable complex autonomous workflows
5. **Path Extraction**: AI can access nested data without complex parsing
6. **Type Preservation**: Original data types maintained through encoding/decoding

Perfect for autonomous AI systems that need to chain screenshots, JS execution, widget inspection, and other debugging commands into comprehensive validation workflows.