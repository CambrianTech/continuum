# WebSocket Module - TypeScript Implementation

Modern WebSocket server with daemon integration and modular architecture.

## 🏗️ Module Structure

```
src/integrations/websocket/
├── package.json              # Module definition
├── README.md                 # This file
├── index.server.js           # Module exports
├── WebSocketServer.ts        # Main server class
├── core/                     # Core components
│   ├── ConnectionManager.ts  # Client connection management
│   ├── MessageRouter.ts      # Message routing and handling
│   └── DaemonConnector.ts    # TypeScript daemon integration
├── handlers/                 # Message handlers
│   ├── CommandHandler.ts     # Command execution handler
│   ├── EventHandler.ts       # Event broadcasting handler
│   └── AuthHandler.ts        # Client authentication handler
├── types/                    # TypeScript interfaces
│   ├── index.ts              # All type exports
│   ├── Connection.ts         # Connection-related types
│   ├── Message.ts            # Message format types
│   └── Daemon.ts             # Daemon integration types
└── test/                     # Unit tests
    ├── WebSocketServer.test.ts
    ├── ConnectionManager.test.ts
    ├── MessageRouter.test.ts
    ├── DaemonConnector.test.ts
    └── integration/
        └── FullSystem.test.ts
```

## 🚀 Features

- **TypeScript-first** - Full type safety and modern architecture
- **Daemon Integration** - Direct connection to TypeScript command daemons
- **Modular Design** - Clean separation of concerns
- **Connection Management** - Robust client lifecycle handling
- **Message Routing** - Flexible message handling system
- **Unit Tested** - Comprehensive test coverage
- **Event-Driven** - Reactive architecture with events

## 📦 Usage

```typescript
import { WebSocketServer } from './WebSocketServer';
import { ConnectionManager } from './core/ConnectionManager';
import { MessageRouter } from './core/MessageRouter';
import { DaemonConnector } from './core/DaemonConnector';

// Start the server
const server = new WebSocketServer({
  port: 9000,
  maxClients: 100
});

await server.start();
```

## 🧪 Testing

```bash
npm test                    # Run all tests
npm test -- --watch        # Watch mode
npm test ConnectionManager # Test specific module
```

## 🔧 Configuration

See `types/Connection.ts` for configuration options.