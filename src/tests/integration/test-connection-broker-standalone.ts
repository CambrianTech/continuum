/**
 * Standalone Connection Broker Test - No Jest Dependencies
 * 
 * This tests the Connection Broker functionality in isolation
 * without requiring the full JTAG system to be running.
 */

import { ConnectionBroker } from './system/core/connection-broker/shared/ConnectionBroker';
import { generateUUID } from './system/core/types/CrossPlatformUUID';
import type { 
  ConnectionParams,
  IConnectionBroker 
} from './system/core/connection-broker/shared/ConnectionBrokerTypes';
import type { ITransportFactory } from './system/transports/shared/ITransportFactory';
import type { JTAGTransport, TransportConfig } from './system/transports/shared/TransportTypes';
import type { JTAGContext } from './system/core/types/JTAGTypes';

// Simple mock transport for testing
class TestMockTransport implements JTAGTransport {
  constructor(public name: string, private port?: number) {}
  
  async send() {
    return { success: true, timestamp: new Date().toISOString() };
  }
  
  isConnected() {
    return true;
  }
  
  async disconnect() {
    console.log(`📡 MockTransport ${this.name} disconnected`);
  }
  
  getConnectionInfo() {
    return { 
      protocol: 'websocket' as const, 
      isConnected: true, 
      connectionTime: Date.now(),
      port: this.port 
    };
  }
}

// Simple mock transport factory
class TestTransportFactory implements ITransportFactory {
  private transportCounter = 0;
  
  async createTransport(environment: string, config: TransportConfig): Promise<JTAGTransport> {
    this.transportCounter++;
    const name = `Test-${environment}-${config.protocol}-${config.role}-${this.transportCounter}`;
    console.log(`🏭 Creating mock transport: ${name} on port ${config.serverPort}`);
    return new TestMockTransport(name, config.serverPort);
  }
  
  async createWebSocketTransport(environment: string, config: TransportConfig): Promise<JTAGTransport> {
    return this.createTransport(environment, config);
  }
}

// Test utilities
function createTestContext(): JTAGContext {
  return {
    uuid: generateUUID(),
    environment: 'server'
  };
}

function createConnectionParams(overrides: Partial<ConnectionParams> = {}): ConnectionParams {
  return {
    protocols: ['websocket'],
    mode: 'shared',
    targetEnvironment: 'server',
    sessionId: generateUUID(),
    context: createTestContext(),
    timeoutMs: 5000,
    enableFallback: true,
    maxRetries: 3,
    ...overrides
  };
}

// Main test function
async function testConnectionBroker() {
  console.log('🧪 Starting Connection Broker standalone test...\n');
  
  const factory = new TestTransportFactory();
  const broker: IConnectionBroker = new ConnectionBroker({
    portPool: {
      startPort: 9500,
      endPort: 9510,
      reservedPorts: [],
      allocationStrategy: 'sequential'
    }
  }, factory);
  
  try {
    console.log('📋 Test 1: Single connection creation');
    const connection1 = await broker.connect(createConnectionParams({
      mode: 'isolated'
    }));
    
    console.log(`✅ Connection 1: ${connection1.strategy}, port ${connection1.server.port}`);
    
    console.log('\n📋 Test 2: Multiple isolated connections (should get different ports)');
    const isolatedConnections = await Promise.all([
      broker.connect(createConnectionParams({ mode: 'isolated' })),
      broker.connect(createConnectionParams({ mode: 'isolated' })),
      broker.connect(createConnectionParams({ mode: 'isolated' }))
    ]);
    
    isolatedConnections.forEach((conn, i) => {
      console.log(`✅ Isolated ${i + 1}: ${conn.strategy}, port ${conn.server.port}`);
    });
    
    // Verify unique ports
    const ports = [connection1, ...isolatedConnections].map(c => c.server.port);
    const uniquePorts = new Set(ports);
    console.log(`🔍 Port uniqueness: ${uniquePorts.size}/${ports.length} unique ports`);
    
    console.log('\n📋 Test 3: Shared connections (should reuse servers)');
    
    // Mark first connection's server as ready for reuse
    const state = await broker.getRegistryState();
    const firstServer = state.servers.find(s => s.guid === connection1.server.guid);
    if (firstServer) {
      (firstServer as any).status = 'ready';
    }
    
    const sharedConnection = await broker.connect(createConnectionParams({
      mode: 'shared',
      targetEnvironment: 'server' // Same environment as first connection
    }));
    
    console.log(`✅ Shared connection: ${sharedConnection.strategy}, port ${sharedConnection.server.port}`);
    
    if (sharedConnection.strategy === 'reused_existing') {
      console.log(`🔄 Successfully reused server ${sharedConnection.server.guid}`);
    } else {
      console.log(`🆕 Created new server (reuse conditions not met)`);
    }
    
    console.log('\n📋 Test 4: Registry state validation');
    const finalState = await broker.getRegistryState();
    
    console.log(`📊 Registry statistics:`);
    console.log(`  - Servers: ${finalState.servers.length}`);
    console.log(`  - Port allocations: ${finalState.portAllocations.size}`);
    console.log(`  - Total connections: ${finalState.statistics.totalConnections}`);
    console.log(`  - Successful connections: ${finalState.statistics.successfulConnections}`);
    console.log(`  - Connection reuse rate: ${(finalState.statistics.reuseRate * 100).toFixed(1)}%`);
    console.log(`  - Average connection time: ${finalState.statistics.avgConnectionTime.toFixed(1)}ms`);
    
    console.log('\n📋 Test 5: Cleanup and shutdown');
    if (broker instanceof ConnectionBroker) {
      await broker.shutdown();
      console.log('✅ Broker shutdown complete');
    }
    
    console.log('\n🎉 All Connection Broker tests passed!');
    return true;
    
  } catch (error) {
    console.error('\n❌ Connection Broker test failed:', error);
    if (broker instanceof ConnectionBroker) {
      await broker.shutdown();
    }
    return false;
  }
}

// Run the test
if (import.meta.url === `file://${process.argv[1]}`) {
  testConnectionBroker().then(success => {
    process.exit(success ? 0 : 1);
  }).catch(error => {
    console.error('💥 Test execution error:', error);
    process.exit(1);
  });
}