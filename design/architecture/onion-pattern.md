# Layered Onion Architecture

## 🧅 Dual Onion System Architecture

Continuum implements a **dual onion architecture** with two independent onion systems that communicate through well-defined interfaces:

### **Server-Side Onion (Core Continuum OS)**
```
┌─────────────────────────────────────────────┐
│                Core System                  │
│  ┌─────────────────────────────────────────┐│
│  │            Command System              ││
│  │  ┌─────────────────────────────────────┐││
│  │  │         Daemon Layer            │││
│  │  │  ┌─────────────────────────────────┐│││
│  │  │  │      Foundation Layer      ││││
│  │  │  │                            ││││
│  │  │  └─────────────────────────────────┘│││
│  │  └─────────────────────────────────────┘││
│  └─────────────────────────────────────────┘│
└─────────────────────────────────────────────┘
```

### **Client-Side Onion (Thin Client APIs)**
```
┌─────────────────────────────────────────────┐
│              Browser Client                 │
│  ┌─────────────────────────────────────────┐│
│  │            Widget Layer             ││
│  │  ┌─────────────────────────────────────┐││
│  │  │         API Bridge Layer        │││
│  │  │  ┌─────────────────────────────────┐│││
│  │  │  │      DOM Interface Layer   ││││
│  │  │  │                            ││││
│  │  │  └─────────────────────────────────┘│││
│  │  └─────────────────────────────────────┘││
│  └─────────────────────────────────────────┘│
└─────────────────────────────────────────────┘
```

## 🔄 How the Two Onions Interact

### **Communication Flow**
```
Browser Widget → API Bridge → WebSocket → Command System → Daemon Layer → Foundation
                                   ↓
                              Cross-onion
                              Communication
                                   ↓
Foundation → Daemon Layer → Command System → WebSocket → API Bridge → Browser Widget
```

### **Interface Contract**
```typescript
// Client-side API Bridge
interface ContinuumAPIBridge {
  executeCommand(name: string, args: any): Promise<CommandResult>;
  subscribeToEvents(callback: (event: SystemEvent) => void): void;
  getSystemHealth(): Promise<HealthStatus>;
}

// Server-side Command Interface
interface CommandSystem {
  registerCommand(name: string, handler: CommandHandler): void;
  executeCommand(name: string, args: any): Promise<CommandResult>;
  broadcastEvent(event: SystemEvent): void;
}
```

## 🏗️ Universal Onion Pattern

### **Dependency Direction (Iron Law)**
```
OUTER layers depend on INNER layers
INNER layers NEVER depend on OUTER layers
```

**Example Dependencies:**
```typescript
// ✅ CORRECT: Widget depends on API Bridge
class ChatWidget {
  constructor(private apiBridge: ContinuumAPIBridge) {}
  
  async sendMessage(message: string) {
    return await this.apiBridge.executeCommand('chat:send', { message });
  }
}

// ❌ WRONG: API Bridge depends on Widget
class APIBridge {
  constructor(private widget: ChatWidget) {} // VIOLATION!
}
```

### **Layer Separation in Practice**

**Foundation Layer** (Core utilities, no dependencies)
```typescript
// src/foundation/utils/
export class Logger {
  static log(message: string): void {
    console.log(`[${new Date().toISOString()}] ${message}`);
  }
}
```

**Daemon Layer** (Depends only on Foundation)
```typescript
// src/daemons/command-processor/
import { Logger } from '../../foundation/utils/Logger';

export class CommandProcessorDaemon {
  constructor() {
    Logger.log('CommandProcessorDaemon initialized');
  }
}
```

**Command Layer** (Depends on Daemon + Foundation)
```typescript
// src/commands/system/health/
import { Logger } from '../../../foundation/utils/Logger';
import { CommandProcessorDaemon } from '../../../daemons/command-processor/CommandProcessorDaemon';

export class HealthCommand {
  static async execute() {
    Logger.log('Executing health command');
    // Implementation
  }
}
```

## 🧩 Universal Module Pattern

### **Mandatory Module Structure**
```
src/[category]/[module]/
├── package.json          # Makes it discoverable by daemon system
├── [Module].ts           # Server implementation  
├── [Module].client.js    # Browser implementation (if needed)
├── test/
│   ├── unit/            # Unit tests
│   │   └── [Module].test.ts
│   └── integration/     # Integration tests
│       └── [Module].integration.test.ts
├── README.md            # Self-documentation
└── assets/              # Module-specific resources (CSS, etc.)
```

**ZERO EXCEPTIONS. NO CROSS-CUTTING DEPENDENCIES. ALL PAYLOADS SELF-CONTAINED.**

### **Module Discovery Example**
```typescript
// Daemon automatically discovers modules by scanning for package.json
async function discoverModules(category: string): Promise<Module[]> {
  const categoryPath = path.join('src', category);
  const subdirs = await fs.readdir(categoryPath);
  
  const modules = [];
  for (const subdir of subdirs) {
    const packagePath = path.join(categoryPath, subdir, 'package.json');
    if (await fs.pathExists(packagePath)) {
      const moduleInfo = await fs.readJson(packagePath);
      modules.push(new Module(subdir, moduleInfo));
    }
  }
  
  return modules;
}
```

## 🔌 Cross-Onion Communication

### **Server-to-Client Events**
```typescript
// Server broadcasts system events
class SystemEventBroadcaster {
  broadcastToClients(event: SystemEvent) {
    this.webSocketServer.clients.forEach(client => {
      client.send(JSON.stringify(event));
    });
  }
}

// Client receives and handles events
class ClientEventHandler {
  handleSystemEvent(event: SystemEvent) {
    switch (event.type) {
      case 'daemon-status-changed':
        this.updateDaemonStatus(event.data);
        break;
      case 'command-completed':
        this.handleCommandCompletion(event.data);
        break;
    }
  }
}
```

### **Client-to-Server Commands**
```typescript
// Client executes commands on server
class ClientCommandExecutor {
  async executeCommand(name: string, args: any): Promise<CommandResult> {
    const message = {
      type: 'command-execution',
      command: name,
      args: args,
      timestamp: Date.now()
    };
    
    return await this.sendMessage(message);
  }
}

// Server processes client commands
class ServerCommandProcessor {
  async processClientCommand(message: CommandMessage): Promise<CommandResult> {
    const command = this.commandRegistry.get(message.command);
    if (!command) {
      throw new Error(`Command not found: ${message.command}`);
    }
    
    return await command.execute(message.args);
  }
}
```

## 🎯 Architecture Benefits

### **Separation of Concerns**
- **Server onion** handles business logic, data processing, system management
- **Client onion** handles user interface, user interactions, visual presentation
- **Clear boundaries** prevent coupling between presentation and business logic

### **Independent Scaling**
- **Server onion** can scale horizontally across multiple processes/machines
- **Client onion** scales per user session in browsers
- **Communication layer** handles load balancing and failover

### **Testability**
- **Each layer** can be tested in isolation
- **Mock interfaces** at layer boundaries enable comprehensive testing
- **Integration tests** verify cross-onion communication

### **Maintainability**
- **Predictable structure** makes navigation and modification easier
- **Dependency direction** prevents circular dependencies and coupling
- **Module discovery** enables automatic system extension

The onion pattern provides a robust, scalable architecture that maintains clean separation of concerns while enabling powerful cross-system communication.