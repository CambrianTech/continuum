# BrowserConsoleDaemon

Browser console capture and forwarding daemon for the Continuum platform.

## Overview

The BrowserConsoleDaemon is a modular component extracted from the monolithic `continuum-browser.ts` (Phase 2 migration). It handles all console capture, forwarding, and queue management functionality with proper session correlation.

## Features

### Console Capture
- **Complete Method Override**: Intercepts ALL console methods (log, info, warn, error, debug, trace, table, group, groupEnd)
- **Deep Argument Inspection**: Analyzes and serializes complex objects, functions, errors, and arrays
- **Source Location Tracking**: Extracts file/line information from stack traces
- **Error Context**: Captures error names, messages, stacks, and causes

### Error Handling
- **Unhandled Error Capture**: Listens for `window.error` events
- **Promise Rejection Capture**: Listens for `unhandledrejection` events
- **Error Count Tracking**: Maintains global error counter
- **Fallback Safety**: Prevents error loops with original console fallbacks

### Queue Management
- **Intelligent Queuing**: Buffers console logs when WebSocket is disconnected
- **Rate Limiting**: 50ms delays between forwards to prevent server overwhelm
- **Session Correlation**: Waits for sessionId before forwarding logs
- **Processing States**: Tracks queue processing state to prevent overlaps

### Browser Context
- **Viewport Information**: Captures current window dimensions
- **User Agent**: Records browser and device information
- **URL Tracking**: Includes current page URL with each log
- **Timestamp Precision**: ISO timestamp with microsecond precision

## Integration

### Feature Flag Control
The daemon respects the `BrowserFeatureFlags.CONSOLE_DAEMON_ENABLED` flag:
- **Default**: `false` (uses legacy monolithic implementation)
- **Development**: Can be enabled via localStorage `continuum_console_daemon=true`
- **Environment**: Can be controlled via `CONTINUUM_CONSOLE_DAEMON=true`

### Message Interface
The daemon handles these message types:
- `console:capture` - Manually trigger console capture
- `console:process_queue` - Force queue processing
- `console:set_session` - Update session ID for log correlation
- `console:get_status` - Get daemon status and metrics
- `console:disable` - Disable console capture
- `console:enable` - Enable console capture

### Event Emission
The daemon emits events via the event bus:
- `console:forward` - Forwards console data to server

## Migration Strategy

### Phase 2 Implementation
1. **Dual Implementation**: New daemon runs alongside legacy code
2. **Feature Flag Control**: Safe rollback to legacy implementation
3. **Gradual Testing**: Enable only in development initially
4. **Session Compatibility**: Maintains existing session correlation

### Testing Commands
```javascript
// Enable console daemon testing (development only)
BrowserFeatureFlags.enableConsoleeDaemonTesting();

// Check daemon status
window.continuum.console.getStatus();

// Disable all browser daemons (emergency rollback)
BrowserFeatureFlags.disableAllFeatures();
```

## Architecture

### Class Structure
```typescript
BrowserConsoleDaemon extends BaseBrowserDaemon {
  // Console method overrides
  private setupConsoleCapture(): void
  private setupErrorCapture(): void
  
  // Data processing
  private forwardConsoleLog(type: string, args: any[]): void
  private inspectArgument(arg: any): any
  private limitDepthInspection(value: any, maxDepth: number): any
  
  // Queue management
  private queueConsoleCommand(command: ConsoleCommand): void
  private processConsoleQueue(): Promise<void>
  
  // Utilities
  private getSourceLocation(stackTrace?: string): string
}
```

### Data Flow
1. **Console Method Called** → Override intercepts
2. **Original Console** → Display in DevTools  
3. **Deep Inspection** → Analyze arguments and context
4. **Queue Command** → Buffer for server forwarding
5. **Rate Limited Processing** → Forward to server via event bus
6. **Session Correlation** → Include sessionId for log files

## Compliance

This module follows the universal modular architecture:
- ✅ **package.json** with `continuum.type: "browser-daemon"`
- ✅ **Main file** `BrowserConsoleDaemon.ts`
- ✅ **README.md** documentation
- ✅ **test/** directory structure
- ✅ **TypeScript** strict mode compliance
- ✅ **ESLint** enforcement ready

## Performance

### Optimizations
- **Argument Inspection Limits**: Max 10 object properties, 5 array elements
- **Depth Limiting**: Prevents infinite recursion in complex objects
- **Rate Limiting**: 50ms delays prevent server overwhelm
- **Queue Batching**: Processes multiple commands efficiently
- **Memory Management**: Bounded queue sizes prevent memory leaks

### Monitoring
- Queue length tracking
- Processing state monitoring
- Error count metrics
- Session correlation status
- Feature flag status reporting

## Security

### Safe Fallbacks
- Original console methods preserved for recovery
- Error loop prevention with try/catch isolation
- Silent failure on inspection errors
- Emergency disable functionality

### Data Privacy
- Deep inspection respects object boundaries
- Sensitive data can be filtered at inspection level
- Session correlation maintains data isolation
- Source location masking for sensitive files