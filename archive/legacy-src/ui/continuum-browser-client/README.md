# Continuum Browser Client

A modular browser client for the Continuum system that provides a single global API for browser-server communication.

## Architecture

This module is structured using clean separation of concerns:

### Core Components

- **ContinuumBrowserClient.ts** - Main API implementation
- **types/BrowserClientTypes.ts** - Browser client specific types and interfaces
- **types/WebSocketTypes.ts** - WebSocket communication protocol types
- **types/ConsoleTypes.ts** - Console forwarding specific types
- **console/ConsoleForwarder.ts** - Console message forwarding with queue management
- **connection/WebSocketManager.ts** - WebSocket connection lifecycle management

### Features

- **Lifecycle-aware API** - Proper state management (initializing → connecting → connected → ready)
- **Console forwarding** - All console messages forwarded to server with session context
- **Message queuing** - Console messages queued during connection setup and flushed when ready
- **Health checking** - Automated validation of console forwarding for all log levels
- **Type safety** - Full TypeScript types, no `any` usage
- **Modular design** - Clean separation of concerns for maintainability

### Usage

```typescript
import continuum from './continuum-browser-client';

// API is available globally as window.continuum
console.log('API version:', continuum.version);
console.log('Connection state:', continuum.state);

// Execute commands
await continuum.execute('health', {});

// Listen for state changes
continuum.onStateChange((state) => {
  console.log('State changed to:', state);
});

// Listen for ready state
continuum.onReady(() => {
  console.log('API is ready to use');
});
```

### State Flow

1. **initializing** - Module loading
2. **connecting** - WebSocket connection establishing
3. **connected** - WebSocket connected, console forwarding enabled
4. **ready** - Session established, command execution available
5. **error** - Connection failed or other error state

### Console Forwarding

All console messages (log, warn, error, info, trace) are automatically forwarded to the server and written to session-specific browser.log files. Messages are queued during connection setup and flushed when the API reaches ready state.

### Testing

The module follows the middle-out testing methodology:

- **Unit tests** - Individual component testing
- **Integration tests** - Cross-component testing
- **Module compliance** - Automated validation via universal test runner

### Module Structure

```
src/ui/continuum-browser-client/
├── package.json                     # Module metadata
├── README.md                        # This file
├── index.ts                         # Module entry point
├── ContinuumBrowserClient.ts        # Main implementation
├── types/
│   ├── BrowserClientTypes.ts       # Browser client specific types
│   ├── WebSocketTypes.ts           # WebSocket communication types
│   └── ConsoleTypes.ts             # Console forwarding types
├── console/
│   └── ConsoleForwarder.ts         # Console message handling
├── connection/
│   └── WebSocketManager.ts         # WebSocket lifecycle
└── test/
    ├── unit/                        # Unit tests
    └── integration/                 # Integration tests
```

This module replaces the monolithic `lifecycle-continuum.ts` with a properly structured, testable, and maintainable modular architecture.