# 🔧 Continuum Daemon System

**Modern TypeScript Daemon Architecture**

## 📋 Overview

The Continuum daemon system provides a robust, event-driven architecture for background services. All daemons are implemented in TypeScript with standardized interfaces, health monitoring, and automatic recovery.

---

## 🏗️ Daemon Architecture

### **Core Components**

#### **BaseDaemon.ts** - Foundation Class
```typescript
export abstract class BaseDaemon extends EventEmitter {
  protected abstract onStart(): Promise<void>;
  protected abstract onStop(): Promise<void>;
  
  // Standardized lifecycle
  async start(): Promise<void>
  async stop(): Promise<void>
  getStatus(): DaemonStatusInfo
  
  // Health monitoring
  emit('heartbeat', status)     // Every 30 seconds
  emit('started')              // On successful startup
  emit('stopped')              // On graceful shutdown
  emit('log', logEntry)        // All log entries
}
```

#### **DaemonManager.ts** - Orchestration
```typescript
export class DaemonManager extends EventEmitter {
  // Dependency-aware startup
  async startAllDaemons(): Promise<void>
  
  // Health monitoring
  getSystemHealth(): SystemHealthStatus
  
  // Auto-restart critical daemons
  private async handleDaemonFailure(daemon: DaemonInfo): Promise<void>
}
```

---

## 🚀 Available Daemons

### **1. CommandProcessorDaemon** (Critical)
**Path:** `src/daemons/command-processor/CommandProcessorDaemon.ts`

**Purpose:** Executes commands with Lambda architecture routing

**Features:**
- Pattern of Care validation
- Multi-provider execution (browser/python/cloud/mesh)
- Automatic fallback chains
- Quality-aware routing

```typescript
interface CommandExecution {
  command: string;
  provider: 'browser' | 'python' | 'cloud' | 'mesh';
  implementation: CommandImplementation;
  quality: 'fast' | 'balanced' | 'accurate';
}
```

**Dependencies:** None (foundational)

### **2. WebSocketDaemon** (Critical)
**Path:** `src/integrations/websocket/WebSocketDaemon.ts`

**Purpose:** WebSocket server for browser communication

**Features:**
- HTTP server on port 9000
- Connection management with heartbeat
- Dynamic message routing
- Static file serving (/src/ and /dist/)

```typescript
interface WebSocketMessage {
  type: string;
  data: any;
  requestId?: string;    // For promise correlation
  timestamp: Date;
}
```

**Dependencies:** `CommandProcessorDaemon`

### **3. RendererDaemon** (Non-Critical)
**Path:** `src/daemons/renderer/RendererDaemon.ts`

**Purpose:** UI rendering and template management

**Features:**
- Clean TypeScript UI templates
- Component serving
- Version management

**Dependencies:** `WebSocketDaemon`

### **4. PersonaDaemon** (Non-Critical)
**Path:** `src/daemons/persona/PersonaDaemon.ts`

**Purpose:** AI persona management and LoRA coordination

**Features:**
- Persona lifecycle management
- LoRA adapter coordination
- Training pipeline integration

**Dependencies:** `CommandProcessorDaemon`

---

## 🔄 Daemon Lifecycle

### **Startup Sequence**
```typescript
// Dependency-ordered startup
const startupOrder = [
  'command-processor',      // Foundation
  'websocket-server',       // Depends on command-processor
  'renderer',              // Depends on websocket-server
  'persona'                // Depends on command-processor
];

for (const daemonName of startupOrder) {
  await this.startDaemon(daemonName);
  await this.waitForHealthy(daemonName);
}
```

### **Health Monitoring**
```typescript
// Automatic health checks every 30 seconds
private startHealthMonitoring(): void {
  setInterval(() => {
    for (const daemon of this.daemons.values()) {
      const status = daemon.getStatus();
      if (status.status === 'failed' && daemon.config.critical) {
        this.restartDaemon(daemon);
      }
    }
  }, 30000);
}
```

### **Graceful Shutdown**
```typescript
// Reverse dependency order shutdown
async shutdown(): Promise<void> {
  const shutdownOrder = startupOrder.reverse();
  
  for (const daemonName of shutdownOrder) {
    await this.stopDaemon(daemonName);
  }
}
```

---

## 📊 Health Check System

### **Health Status Types**
```typescript
type DaemonStatus = 'starting' | 'running' | 'stopping' | 'stopped' | 'failed';

interface DaemonStatusInfo {
  name: string;
  version: string;
  status: DaemonStatus;
  pid: number;
  startTime?: Date;
  lastHeartbeat?: Date;
  uptime: number;
  memoryUsage: NodeJS.MemoryUsage;
  cpuUsage: NodeJS.CpuUsage;
}
```

### **System Health Assessment**
```typescript
interface SystemHealthStatus {
  overall: 'HEALTHY' | 'DEGRADED' | 'CRITICAL';
  daemons: DaemonStatusInfo[];
  startTime: Date;
  systemUptime: number;
  criticalDaemonsUp: number;
  totalDaemons: number;
}
```

### **Health Check Commands**
```bash
# Check daemon status
python python-client/ai-portal.py --daemons

# Get specific daemon logs  
python python-client/ai-portal.py --daemon-logs websocket-server

# System health check
python python-client/ai-portal.py --cmd selftest

# Emergency recovery
python python-client/ai-portal.py --failsafe
```

---

## 🔌 Inter-Daemon Communication

### **Message Protocol**
```typescript
interface DaemonMessage {
  id: string;
  from: string;
  to: string;
  type: string;
  data: any;
  timestamp: Date;
  priority?: 'low' | 'normal' | 'high' | 'critical';
}

interface DaemonResponse {
  success: boolean;
  data?: any;
  error?: string;
  messageId?: string;
  processingTime?: number;
}
```

### **Event Broadcasting**
```typescript
// Daemons can subscribe to events from other daemons
class PersonaDaemon extends BaseDaemon {
  protected async onStart(): Promise<void> {
    // Subscribe to command processor events
    this.daemonManager.subscribe('command-processor', 'command-executed', 
      this.handleCommandExecution.bind(this));
  }
}
```

---

## 🛠️ Creating New Daemons

### **1. Daemon Implementation**
```typescript
// src/daemons/my-daemon/MyDaemon.ts
import { BaseDaemon } from '../base/BaseDaemon.js';

export class MyDaemon extends BaseDaemon {
  public readonly name = 'my-daemon';
  public readonly version = '1.0.0';

  protected async onStart(): Promise<void> {
    // Daemon initialization
    this.log('Starting MyDaemon...');
    
    // Setup services
    await this.initializeServices();
    
    this.log('MyDaemon started successfully');
  }

  protected async onStop(): Promise<void> {
    // Cleanup
    await this.cleanupServices();
    this.log('MyDaemon stopped');
  }

  // Public API methods
  async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
    switch (message.type) {
      case 'my-operation':
        return await this.performOperation(message.data);
      default:
        return { success: false, error: 'Unknown message type' };
    }
  }
}
```

### **2. Daemon Configuration**
```typescript
// src/daemons/my-daemon/MyDaemonConfig.ts
export interface MyDaemonConfig {
  port?: number;
  timeout?: number;
  maxConnections?: number;
  enableFeatureX?: boolean;
}

export const defaultConfig: MyDaemonConfig = {
  port: 3000,
  timeout: 30000,
  maxConnections: 100,
  enableFeatureX: true
};
```

### **3. Package.json**
```json
{
  "name": "@continuum/my-daemon",
  "version": "1.0.0",
  "description": "My custom Continuum daemon",
  "main": "MyDaemon.ts",
  "continuum": {
    "type": "daemon",
    "category": "service",
    "critical": false,
    "dependencies": ["command-processor"],
    "capabilities": ["my-operation", "data-processing"],
    "autoStart": true,
    "autoRestart": true
  }
}
```

### **4. Registration**
```typescript
// Add to DaemonManager.ts
const daemonConfigs: DaemonConfig[] = [
  // ... existing daemons
  {
    name: 'my-daemon',
    module: () => import('./my-daemon/MyDaemon.js'),
    dependencies: ['command-processor'],
    critical: false,
    autoRestart: true
  }
];
```

---

## 🔧 Development Tools

### **Daemon Testing**
```typescript
// src/daemons/my-daemon/test/MyDaemon.test.ts
import { MyDaemon } from '../MyDaemon.js';

describe('MyDaemon', () => {
  let daemon: MyDaemon;

  beforeEach(async () => {
    daemon = new MyDaemon();
    await daemon.start();
  });

  afterEach(async () => {
    await daemon.stop();
  });

  test('should handle messages correctly', async () => {
    const result = await daemon.handleMessage({
      id: 'test-1',
      from: 'test',
      to: 'my-daemon',
      type: 'my-operation',
      data: { test: 'data' },
      timestamp: new Date()
    });

    expect(result.success).toBe(true);
  });
});
```

### **Daemon Monitoring**
```bash
# Real-time daemon status
watch -n 2 'python python-client/ai-portal.py --daemons'

# Daemon log streaming
python python-client/ai-portal.py --daemon-logs my-daemon --follow

# Performance monitoring
python python-client/ai-portal.py --cmd diagnostics --params '{"daemon": "my-daemon"}'
```

---

## 🎯 Best Practices

### **1. Error Handling**
```typescript
protected async onStart(): Promise<void> {
  try {
    await this.initializeServices();
  } catch (error) {
    this.log(`Failed to start: ${error.message}`, 'error');
    throw error; // Let DaemonManager handle restart
  }
}
```

### **2. Resource Management**
```typescript
private connections = new Set<Connection>();

async addConnection(conn: Connection): Promise<void> {
  this.connections.add(conn);
  conn.on('close', () => this.connections.delete(conn));
}

protected async onStop(): Promise<void> {
  // Cleanup all connections
  for (const conn of this.connections) {
    await conn.close();
  }
  this.connections.clear();
}
```

### **3. Performance Monitoring**
```typescript
private performanceMetrics = {
  messagesProcessed: 0,
  averageProcessingTime: 0,
  errorCount: 0
};

async handleMessage(message: DaemonMessage): Promise<DaemonResponse> {
  const startTime = Date.now();
  
  try {
    const result = await this.processMessage(message);
    this.updateMetrics(Date.now() - startTime, true);
    return result;
  } catch (error) {
    this.updateMetrics(Date.now() - startTime, false);
    throw error;
  }
}
```

---

## 🚨 Troubleshooting

### **Common Issues**

1. **Daemon Won't Start**
   ```bash
   # Check dependencies
   python python-client/ai-portal.py --daemon-status command-processor
   
   # Check logs
   python python-client/ai-portal.py --daemon-logs my-daemon
   ```

2. **High Memory Usage**
   ```bash
   # Monitor resource usage
   python python-client/ai-portal.py --cmd diagnostics --params '{"type": "memory"}'
   ```

3. **Communication Issues**
   ```bash
   # Test message routing
   python python-client/ai-portal.py --cmd test_daemon_communication
   ```

### **Emergency Recovery**
```bash
# Full system restart
python python-client/ai-portal.py --failsafe

# Individual daemon restart
python python-client/ai-portal.py --restart-daemon my-daemon

# Health check and auto-repair
python python-client/ai-portal.py --cmd selftest --params '{"repair": true}'
```

This daemon system provides a robust foundation for building scalable, maintainable background services with built-in monitoring, recovery, and communication capabilities.