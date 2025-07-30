# JTAG Transport System

The JTAG Transport System provides a flexible, role-based networking layer that supports everything from simple client-server architectures to complex peer-to-peer mesh networks.

## 🏗️ Architecture Overview

The transport system is built around **roles** that define connection behavior, not just environment context. This enables the same codebase to work in various network topologies.

### Core Components

```
system/transports/
├── shared/                    # Core types and factory
│   ├── TransportTypes.ts      # Role definitions and interfaces
│   ├── TransportFactory.ts    # Universal transport creator
│   └── TransportBase.ts       # Base implementation
├── websocket/                 # WebSocket transport implementation
│   ├── client/                # WebSocket client transport
│   ├── server/                # WebSocket server transport
│   └── shared/                # Shared WebSocket utilities
├── http/                      # HTTP transport implementation
└── index.ts                   # Public API exports
```

## 🎭 Transport Roles

Transport roles define the connection behavior and networking capabilities:

### `CLIENT` - Outbound Connector
**Purpose**: Initiates connections to servers  
**Used by**: CLI tools, browser clients  
**Pattern**: One-to-one (client → server)

```typescript
import { JTAGClient } from '@shared/JTAGClient';

// CLI connecting to running JTAG system
const client = await JTAGClient.connect({
  transportType: 'websocket',
  serverPort: 9001
});
// Automatically uses role: 'client'
```

### `SERVER` - Inbound Listener  
**Purpose**: Accepts connections from multiple clients  
**Used by**: Main JTAG system, service endpoints  
**Pattern**: One-to-many (server ← multiple clients)

```typescript
import { JTAGRouter } from '@shared/JTAGRouter';
import { TRANSPORT_ROLES } from '@systemTransports';

// Server router listening for connections
const router = new JTAGRouter(context, {
  transport: {
    role: TRANSPORT_ROLES.SERVER,
    preferred: 'websocket',
    serverPort: 9001
  }
});
```

### `PEER` - Mesh Network Node
**Purpose**: Bidirectional communication in distributed systems  
**Used by**: P2P mesh nodes, distributed development environments  
**Pattern**: Many-to-many (peer ↔ peer ↔ peer)

```typescript
import { TransportFactory, TRANSPORT_ROLES } from '@systemTransports';

// Peer node in mesh network
const peerTransport = await TransportFactory.createTransport('server', {
  role: TRANSPORT_ROLES.PEER,
  preferred: 'udp-multicast',
  p2p: {
    nodeId: 'developer-laptop',
    nodeType: 'server',
    capabilities: ['screenshot', 'file-ops', 'compilation'],
    multicastAddress: '239.255.255.250',
    multicastPort: 9003
  }
});
```

### `RELAY` - Network Bridge
**Purpose**: Routes messages between different network segments  
**Used by**: Gateway nodes, network bridges, protocol converters  
**Pattern**: Hub pattern (multiple ← relay → multiple)

```typescript
// Gateway that bridges WebSocket clients with UDP mesh peers
const relayTransport = await TransportFactory.createTransport('server', {
  role: TRANSPORT_ROLES.RELAY,
  preferred: 'websocket',
  serverPort: 9001,
  p2p: {
    nodeId: 'gateway-relay',
    multicastPort: 9003
  }
});
```

### `HYBRID` - Multi-Role Node
**Purpose**: Combines client and server capabilities simultaneously  
**Used by**: Advanced distributed architectures, full mesh nodes  
**Pattern**: Full mesh capabilities (can connect and accept)

```typescript
// Node that acts as both client and server
const hybridTransport = await TransportFactory.createTransport('server', {
  role: TRANSPORT_ROLES.HYBRID,
  preferred: 'websocket',
  serverPort: 9001,    // Listen for inbound connections
  p2p: {
    nodeId: 'hybrid-node',
    capabilities: ['screenshot', 'compilation', 'relay']
  }
});
```

## 🚀 Quick Start Examples

### Basic Client-Server Setup

**Server (JTAG System)**:
```typescript
import { JTAGRouter } from '@shared/JTAGRouter';

const router = new JTAGRouter(context); // Defaults to SERVER role
await router.initialize();
// Now listening on port 9001
```

**Client (CLI Tool)**:
```typescript
import { JTAGClient } from '@shared/JTAGClient';

const client = await JTAGClient.connect(); // Defaults to CLIENT role
await client.commands.screenshot({ filename: 'test.png' });
```

### Browser Integration

**Browser (Always Client)**:
```typescript
import { JTAGSystemBrowser } from '@browser/JTAGSystemBrowser';

const system = await JTAGSystemBrowser.connect();
// Browser router automatically uses CLIENT role to connect to server
```

### P2P Mesh Network

**Mesh Node**:
```typescript
import { TransportFactory, TRANSPORT_ROLES } from '@systemTransports';

// Each development machine becomes a mesh peer
const meshTransport = await TransportFactory.createTransport('server', {
  role: TRANSPORT_ROLES.PEER,
  preferred: 'udp-multicast',
  p2p: {
    nodeId: process.env.HOSTNAME,
    nodeType: 'server',
    capabilities: ['screenshot', 'file-ops', 'tests'],
    multicastAddress: '239.255.255.250',
    multicastPort: 9003
  }
});

// Commands can now execute on any mesh node
await meshTransport.send({
  endpoint: 'remote/build-server/commands/compile',
  payload: { language: 'typescript', files: ['src/**/*.ts'] }
});
```

## 🔧 Configuration

### TransportConfig Interface

```typescript
interface TransportConfig {
  preferred?: 'websocket' | 'http' | 'udp-multicast';
  fallback?: boolean;
  role?: TransportRole;
  serverPort?: number;
  serverUrl?: string;
  eventSystem?: EventsInterface;
  sessionId?: string;
  
  // P2P mesh configuration
  p2p?: {
    nodeId?: string;
    nodeType?: 'server' | 'browser' | 'mobile' | 'ai-agent';
    capabilities?: string[];
    multicastAddress?: string;
    multicastPort?: number;
    unicastPort?: number;
    encryptionKey?: string;
  };
}
```

### Default Configurations

**Client Defaults** (JTAGClient):
```typescript
{
  role: TRANSPORT_ROLES.CLIENT,
  preferred: 'websocket',
  serverPort: 9001,
  serverUrl: 'ws://localhost:9001',
  fallback: true
}
```

**Server Defaults** (JTAGRouter):
```typescript
{
  role: TRANSPORT_ROLES.SERVER,
  preferred: 'websocket',
  serverPort: 9001,
  fallback: true
}
```

**Peer Defaults** (P2P Mesh):
```typescript
{
  role: TRANSPORT_ROLES.PEER,
  preferred: 'udp-multicast',
  p2p: {
    multicastAddress: '239.255.255.250',
    multicastPort: 9003,
    unicastPort: 9004
  }
}
```

## 🌐 Network Topologies

### 1. Simple Client-Server
```
CLI ──→ Server JTAG System
Browser ──→ Server JTAG System
```

### 2. Hub and Spoke
```
     CLI ──→ Server ←── Browser
              ↑
        Mobile Client
```

### 3. P2P Mesh Network
```
Laptop ←──→ Desktop ←──→ Server
  ↑           ↑          ↑
  └──→ Build Machine ←──┘
```

### 4. Hybrid Architecture
```
Gateway Relay
├── WebSocket Server (port 9001)
├── UDP Mesh Peer (port 9003)  
└── HTTP Fallback (port 9002)

Clients ──→ Gateway ←──→ Mesh Peers
```

## 🔍 Transport Selection Logic

The TransportFactory automatically selects the best transport:

1. **Role-based selection**: Different roles may prefer different transports
2. **Environment adaptation**: Browser constraints (no UDP server capability)
3. **Fallback chain**: WebSocket → HTTP → Local (with graceful degradation)
4. **P2P discovery**: UDP multicast for automatic mesh formation

```typescript
// Factory decides best transport for role and environment
const transport = await TransportFactory.createTransport(environment, {
  role: TRANSPORT_ROLES.PEER,
  preferred: 'udp-multicast',
  fallback: true  // Falls back to WebSocket if UDP fails
});
```

## 🚦 Connection Lifecycle

### Client Connection
1. **Discovery**: Find available servers (port scan, multicast discovery)
2. **Connection**: Establish transport connection
3. **Handshake**: Exchange capabilities and session info
4. **Ready**: Begin message exchange

### Server Initialization  
1. **Bind**: Listen on configured port
2. **Advertise**: Announce availability (multicast beacon)
3. **Accept**: Handle incoming connections
4. **Route**: Forward messages to appropriate handlers

### Peer Mesh Formation
1. **Broadcast**: Send multicast discovery beacons
2. **Discover**: Receive beacons from other peers
3. **Connect**: Establish direct connections to discovered peers
4. **Maintain**: Keep-alive and reconnection logic

## 🔒 Security Considerations

### Authentication
- **Session-based**: Client provides sessionId for verification
- **Capability-based**: Peers advertise and verify capabilities
- **Environment isolation**: Different security models for browser vs server

### Encryption
- **WebSocket**: TLS/SSL support for secure connections
- **P2P Mesh**: Optional pre-shared key encryption
- **HTTP**: HTTPS support with certificate validation

### Network Security
- **Port binding**: Configurable port ranges to avoid conflicts
- **Multicast scope**: Limited to local network by default
- **Firewall-friendly**: Configurable ports and protocols

## 🛠️ Development and Testing

### Unit Testing
```typescript
import { TransportFactory, TRANSPORT_ROLES } from '@systemTransports';

describe('Transport Factory', () => {
  it('creates client transport', async () => {
    const transport = await TransportFactory.createTransport('server', {
      role: TRANSPORT_ROLES.CLIENT,
      serverUrl: 'ws://localhost:9001'
    });
    expect(transport.name).toBe('websocket-client');
  });
});
```

### Integration Testing
```typescript
// Test client-server communication
const server = await createServerTransport({ role: TRANSPORT_ROLES.SERVER });
const client = await createClientTransport({ role: TRANSPORT_ROLES.CLIENT });

await server.start();
await client.connect();

const response = await client.send(testMessage);
expect(response.success).toBe(true);
```

### P2P Testing
```typescript
// Test mesh network formation
const peers = await Promise.all([
  createPeerTransport({ nodeId: 'peer1' }),
  createPeerTransport({ nodeId: 'peer2' }),
  createPeerTransport({ nodeId: 'peer3' })
]);

// Wait for mesh formation
await waitForMeshFormation(peers);

// Test message routing
const response = await peers[0].send({
  endpoint: 'remote/peer2/commands/ping'
});
```

## 📚 API Reference

### TransportFactory
```typescript
class TransportFactory {
  static async createTransport(
    environment: JTAGEnvironment,
    config: TransportConfig
  ): Promise<JTAGTransport>;
  
  static detectOptimalConfig(
    environment: JTAGEnvironment
  ): TransportConfig;
}
```

### JTAGTransport Interface
```typescript
interface JTAGTransport {
  name: string;
  send(message: JTAGMessage): Promise<TransportSendResult>;
  isConnected(): boolean;
  disconnect(): Promise<void>;
  reconnect?(): Promise<void>;
  setMessageHandler?(handler: (message: JTAGMessage) => void): void;
}
```

### Transport Events
```typescript
const TRANSPORT_EVENTS = {
  CONNECTED: 'transport:connected',
  DISCONNECTED: 'transport:disconnected',
  ERROR: 'transport:error',
  MESSAGE_RECEIVED: 'transport:message:received',
  MESSAGE_SENT: 'transport:message:sent',
  PEER_DISCOVERED: 'transport:peer:discovered',
  PEER_LOST: 'transport:peer:lost'
};
```

## 🔮 Future Enhancements

### Planned Features
- **Load Balancing**: Automatic distribution across mesh peers
- **Circuit Breakers**: Fault tolerance and recovery
- **Message Queuing**: Persistent message delivery
- **Compression**: Automatic payload compression for large messages
- **Metrics**: Transport performance monitoring

### Advanced Topologies
- **Federation**: Connect multiple mesh networks
- **Hierarchical**: Multi-level routing architectures  
- **Mobile Integration**: Cellular and WiFi-aware routing
- **Cloud Bridges**: Connect on-premise with cloud resources

## 🐛 Troubleshooting

### Common Issues

**Connection Refused**:
```typescript
// Check if server is running
const ports = await checkPorts([9001, 9002]);
console.log('Available ports:', ports);
```

**Mesh Discovery Fails**:
```typescript
// Verify multicast configuration
const config = {
  p2p: {
    multicastAddress: '239.255.255.250', // Ensure valid multicast range
    multicastPort: 9003
  }
};
```

**Firewall Blocking**:
```bash
# Allow JTAG ports
sudo ufw allow 9001/tcp  # WebSocket
sudo ufw allow 9002/tcp  # HTTP  
sudo ufw allow 9003/udp  # P2P multicast
```

### Debug Logging
```typescript
import { TransportFactory } from '@systemTransports';

const transport = await TransportFactory.createTransport('server', {
  role: TRANSPORT_ROLES.CLIENT,
  enableDebugLogging: true  // Verbose transport logs
});
```

---

## 📄 License

This transport system is part of the JTAG Universal Command Bus architecture and follows the same licensing terms as the parent project.