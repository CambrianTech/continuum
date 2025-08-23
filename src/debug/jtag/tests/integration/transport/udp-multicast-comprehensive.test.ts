/**
 * UDP Multicast Transport Comprehensive Tests
 * 
 * Deep testing of The Grid's P2P mesh networking transport layer.
 * Tests real UDP sockets, multicast discovery, node routing, and fault tolerance.
 */

import { spawn, ChildProcess } from 'child_process';
import { createTestId } from '../../test-utils/TestIdGenerator';
import { UDPMulticastTransportServer } from '../../../system/transports/udp-multicast-transport/server/UDPMulticastTransportServer';
import { 
  NodeType, 
  NodeCapability, 
  UDP_MULTICAST_DEFAULTS,
  type UDPMulticastConfig
} from '../../../system/transports/udp-multicast-transport/shared/UDPMulticastTypes';
import { generateUUID } from '../../../system/core/types/CrossPlatformUUID';
import dgram from 'dgram';

interface TestTransport {
  transport: UDPMulticastTransportServer;
  nodeId: string;
  config: UDPMulticastConfig;
}

/**
 * UDP Transport Test Suite
 */
export class UDPTransportTestSuite {
  private transports: TestTransport[] = [];
  private testId: string;
  private testPorts: number[] = [];

  constructor() {
    this.testId = createTestId('udp-transport');
    console.log(`🧪 UDP Transport Test Suite: ${this.testId}`);
  }

  /**
   * Generate available test ports
   */
  private generateTestPorts(count: number): number[] {
    const ports: number[] = [];
    let basePort = 45000 + Math.floor(Math.random() * 5000);
    
    for (let i = 0; i < count; i++) {
      ports.push(basePort + i);
    }
    
    this.testPorts = ports;
    return ports;
  }

  /**
   * Create test transport with unique configuration
   */
  private async createTestTransport(
    nodeType: NodeType = NodeType.SERVER,
    capabilities: NodeCapability[] = [NodeCapability.FILE_OPERATIONS],
    unicastPort?: number
  ): Promise<TestTransport> {
    const nodeId = generateUUID();
    const port = unicastPort || this.generateTestPorts(1)[0];
    
    const config: UDPMulticastConfig = {
      nodeId,
      nodeType,
      capabilities,
      multicastAddress: UDP_MULTICAST_DEFAULTS.MULTICAST_ADDRESS,
      multicastPort: UDP_MULTICAST_DEFAULTS.MULTICAST_PORT,
      unicastPort: port,
      ttl: UDP_MULTICAST_DEFAULTS.TTL,
      discoveryInterval: 5000,     // Faster for testing
      heartbeatInterval: 2000,     // Faster for testing
      nodeTimeout: 10000           // Shorter timeout for testing
    };

    const transport = new UDPMulticastTransportServer(config);
    
    const testTransport: TestTransport = {
      transport,
      nodeId,
      config
    };

    this.transports.push(testTransport);
    
    console.log(`📡 Created test transport ${nodeId.substring(0, 8)} on port ${port}`);
    return testTransport;
  }

  /**
   * Test 1: Transport Initialization
   */
  async testTransportInitialization(): Promise<boolean> {
    console.log(`\\n🧪 Test 1: Transport Initialization`);
    
    try {
      // Create a single transport
      const testTransport = await this.createTestTransport();
      
      // Initialize the transport
      console.log('🚀 Initializing transport...');
      await testTransport.transport.initialize();
      
      // Verify initialization
      if (testTransport.transport.connected) {
        console.log('✅ Transport initialized successfully');
        
        // Test basic functionality
        const stats = testTransport.transport.getStats();
        console.log('📊 Initial stats:', JSON.stringify(stats, null, 2));
        
        return true;
      } else {
        console.log('❌ Transport failed to connect');
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ Transport initialization failed:', error.message);
      return false;
    }
  }

  /**
   * Test 2: Node Discovery Protocol  
   */
  async testNodeDiscovery(): Promise<boolean> {
    console.log(`\\n🧪 Test 2: Node Discovery Protocol`);
    
    try {
      // Create multiple transports
      const ports = this.generateTestPorts(3);
      const transports = await Promise.all([
        this.createTestTransport(NodeType.SERVER, [NodeCapability.FILE_OPERATIONS], ports[0]),
        this.createTestTransport(NodeType.SERVER, [NodeCapability.SCREENSHOT], ports[1]),  
        this.createTestTransport(NodeType.SERVER, [NodeCapability.COMPILATION], ports[2])
      ]);
      
      // Initialize all transports
      console.log('🚀 Initializing multiple transports...');
      await Promise.all(transports.map(t => t.transport.initialize()));
      
      // Wait for discovery
      console.log('⏳ Waiting for node discovery...');
      await new Promise(resolve => setTimeout(resolve, 8000));
      
      // Check discovery results
      let totalDiscovered = 0;
      for (const transport of transports) {
        const stats = transport.transport.getStats();
        console.log(`📡 Node ${transport.nodeId.substring(0, 8)}: discovered ${stats.nodesDiscovered} peers`);
        totalDiscovered += stats.nodesDiscovered;
      }
      
      // Each node should discover 2 peers (the other nodes)
      const expectedTotal = (transports.length - 1) * transports.length;
      if (totalDiscovered >= expectedTotal) {
        console.log(`✅ Node discovery successful: ${totalDiscovered}/${expectedTotal} discoveries`);
        return true;
      } else {
        console.log(`❌ Node discovery incomplete: ${totalDiscovered}/${expectedTotal} discoveries`);
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ Node discovery test failed:', error.message);
      return false;
    }
  }

  /**
   * Test 3: P2P Message Routing
   */
  async testMessageRouting(): Promise<boolean> {
    console.log(`\\n🧪 Test 3: P2P Message Routing`);
    
    try {
      // Use existing transports or create new ones
      if (this.transports.length < 2) {
        const ports = this.generateTestPorts(2);
        await Promise.all([
          this.createTestTransport(NodeType.SERVER, [NodeCapability.FILE_OPERATIONS], ports[0]),
          this.createTestTransport(NodeType.SERVER, [NodeCapability.SCREENSHOT], ports[1])
        ]);
      }
      
      const sourceTransport = this.transports[0];
      const targetTransport = this.transports[1];
      
      // Set up message handler on target
      let messageReceived = false;
      let receivedMessage: any = null;
      
      targetTransport.transport.onMessage((message) => {
        console.log(`📥 Target received message:`, message);
        messageReceived = true;
        receivedMessage = message;
      });
      
      // Send message from source to target
      const testMessage = {
        id: generateUUID(),
        command: 'ping',
        payload: { 
          message: 'P2P routing test',
          timestamp: new Date().toISOString()
        },
        timestamp: new Date().toISOString()
      };
      
      console.log(`📤 Sending message from ${sourceTransport.nodeId.substring(0, 8)} to ${targetTransport.nodeId.substring(0, 8)}`);
      
      const result = await sourceTransport.transport.send(testMessage);
      
      // Wait for message propagation
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      if (result.success && messageReceived && receivedMessage) {
        console.log('✅ P2P message routing successful');
        console.log('📋 Message verification:', {
          sent: testMessage.payload.message,
          received: receivedMessage.payload?.message,
          match: testMessage.payload.message === receivedMessage.payload?.message
        });
        return true;
      } else {
        console.log('❌ P2P message routing failed');
        console.log('🔍 Debug info:', {
          sendResult: result.success,
          messageReceived,
          receivedMessage: !!receivedMessage
        });
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ P2P message routing test failed:', error.message);
      return false;
    }
  }

  /**
   * Test 4: Network Fault Tolerance
   */
  async testFaultTolerance(): Promise<boolean> {
    console.log(`\\n🧪 Test 4: Network Fault Tolerance`);
    
    try {
      // Create 3+ transports for fault tolerance testing
      if (this.transports.length < 3) {
        const ports = this.generateTestPorts(3);
        await Promise.all([
          this.createTestTransport(NodeType.SERVER, [NodeCapability.FILE_OPERATIONS], ports[0]),
          this.createTestTransport(NodeType.SERVER, [NodeCapability.SCREENSHOT], ports[1]),
          this.createTestTransport(NodeType.SERVER, [NodeCapability.COMPILATION], ports[2])
        ]);
      }
      
      const [nodeA, nodeB, nodeC] = this.transports.slice(0, 3);
      
      // Verify all nodes are connected
      console.log('🔍 Initial connectivity check...');
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      // Simulate node failure by disconnecting nodeB
      console.log(`💀 Simulating failure of node ${nodeB.nodeId.substring(0, 8)}`);
      await nodeB.transport.disconnect();
      
      // Wait for failure detection
      await new Promise(resolve => setTimeout(resolve, 5000));
      
      // Test routing between remaining nodes (A → C)
      let routingWorked = false;
      nodeC.transport.onMessage((message) => {
        console.log(`📥 Message received after node failure:`, message);
        routingWorked = true;
      });
      
      const testMessage = {
        id: generateUUID(),
        command: 'fault-tolerance-test',
        payload: { 
          message: 'Testing fault tolerance',
          timestamp: new Date().toISOString()
        },
        timestamp: new Date().toISOString()
      };
      
      console.log(`📤 Testing routing after node failure: A → C`);
      const result = await nodeA.transport.send(testMessage);
      
      // Wait for message propagation
      await new Promise(resolve => setTimeout(resolve, 2000));
      
      if (result.success && routingWorked) {
        console.log('✅ Network fault tolerance successful');
        return true;
      } else {
        console.log('❌ Network fault tolerance failed');
        console.log('🔍 Debug info:', {
          sendResult: result.success,
          routingWorked
        });
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ Network fault tolerance test failed:', error.message);
      return false;
    }
  }

  /**
   * Test 5: Load and Performance
   */
  async testLoadAndPerformance(): Promise<boolean> {
    console.log(`\\n🧪 Test 5: Load and Performance`);
    
    try {
      if (this.transports.length < 2) {
        const ports = this.generateTestPorts(2);
        await Promise.all([
          this.createTestTransport(NodeType.SERVER, [NodeCapability.FILE_OPERATIONS], ports[0]),
          this.createTestTransport(NodeType.SERVER, [NodeCapability.SCREENSHOT], ports[1])
        ]);
      }
      
      const [sourceNode, targetNode] = this.transports.slice(0, 2);
      
      // Send multiple messages rapidly
      const messageCount = 20;
      const startTime = Date.now();
      let messagesReceived = 0;
      
      targetNode.transport.onMessage((message) => {
        messagesReceived++;
      });
      
      console.log(`📤 Sending ${messageCount} messages rapidly...`);
      
      const sendPromises = [];
      for (let i = 0; i < messageCount; i++) {
        const message = {
          id: generateUUID(),
          command: 'load-test',
          payload: { 
            sequence: i,
            message: \`Load test message \${i}\`,
            timestamp: new Date().toISOString()
          },
          timestamp: new Date().toISOString()
        };
        
        sendPromises.push(sourceNode.transport.send(message));
      }
      
      const results = await Promise.all(sendPromises);
      const sendTime = Date.now() - startTime;
      
      // Wait for all messages to propagate
      await new Promise(resolve => setTimeout(resolve, 3000));
      
      const successfulSends = results.filter(r => r.success).length;
      const latency = sendTime / messageCount;
      
      console.log('📊 Load test results:');
      console.log(\`  Messages sent: \${successfulSends}/\${messageCount}\`);
      console.log(\`  Messages received: \${messagesReceived}/\${messageCount}\`);
      console.log(\`  Average latency: \${latency.toFixed(2)}ms\`);
      console.log(\`  Total time: \${sendTime}ms\`);
      
      // Success criteria: >= 90% success rate
      const successRate = (messagesReceived / messageCount) * 100;
      if (successRate >= 90) {
        console.log(\`✅ Load and performance test passed (\${successRate.toFixed(1)}% success rate)\`);
        return true;
      } else {
        console.log(\`❌ Load and performance test failed (\${successRate.toFixed(1)}% success rate)\`);
        return false;
      }
      
    } catch (error: any) {
      console.error('❌ Load and performance test failed:', error.message);
      return false;
    }
  }

  /**
   * Clean up all test transports
   */
  async cleanup(): Promise<void> {
    console.log('\\n🧹 Cleaning up UDP transport test suite...');
    
    for (const testTransport of this.transports) {
      try {
        await testTransport.transport.disconnect();
        console.log(\`✅ Cleaned up transport \${testTransport.nodeId.substring(0, 8)}\`);
      } catch (error: any) {
        console.warn(\`⚠️ Error cleaning up transport \${testTransport.nodeId.substring(0, 8)}: \${error.message}\`);
      }
    }
    
    this.transports = [];
    this.testPorts = [];
    console.log('✅ UDP transport test cleanup complete');
  }
}

/**
 * Run comprehensive UDP transport tests
 */
async function runUDPTransportTests(): Promise<void> {
  console.log('🚀 UDP MULTICAST TRANSPORT COMPREHENSIVE TESTS');
  console.log('================================================');
  
  const testSuite = new UDPTransportTestSuite();
  const results: boolean[] = [];
  
  try {
    // Test 1: Transport Initialization
    results.push(await testSuite.testTransportInitialization());
    
    // Test 2: Node Discovery Protocol
    results.push(await testSuite.testNodeDiscovery());
    
    // Test 3: P2P Message Routing
    results.push(await testSuite.testMessageRouting());
    
    // Test 4: Network Fault Tolerance
    results.push(await testSuite.testFaultTolerance());
    
    // Test 5: Load and Performance
    results.push(await testSuite.testLoadAndPerformance());
    
  } finally {
    await testSuite.cleanup();
  }
  
  // Results summary
  console.log('\\n📊 UDP TRANSPORT TEST RESULTS:');
  console.log('=================================');
  
  const testNames = [
    'Transport Initialization',
    'Node Discovery Protocol', 
    'P2P Message Routing',
    'Network Fault Tolerance',
    'Load and Performance'
  ];
  
  const passed = results.filter(r => r).length;
  const total = results.length;
  
  results.forEach((result, index) => {
    const status = result ? '✅ PASSED' : '❌ FAILED';
    console.log(\`  \${testNames[index]}: \${status}\`);
  });
  
  console.log(\`\\n📈 Summary: \${passed}/\${total} tests passed (\${((passed/total)*100).toFixed(1)}%)\`);
  
  if (passed === total) {
    console.log('🎉 All UDP transport tests PASSED!');
    console.log('🌐 The Grid transport layer is ready for mesh networking');
    process.exit(0);
  } else {
    console.log('💥 Some UDP transport tests FAILED!');
    console.log('⚠️ The Grid transport layer needs attention');
    process.exit(1);
  }
}

// Run tests if executed directly
if (require.main === module) {
  runUDPTransportTests().catch(error => {
    console.error('❌ UDP transport test suite crashed:', error);
    process.exit(1);
  });
}

export { UDPTransportTestSuite };