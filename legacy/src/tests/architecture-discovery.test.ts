/**
 * Architecture Discovery Test
 * 
 * Tests to understand what's actually working in the current system.
 * Run this first before making any architecture changes.
 */

import { UDPMulticastTransportServer } from '../system/transports/udp-multicast-transport/server/UDPMulticastTransportServer';
import { NodeType, NodeCapability, UDP_MULTICAST_DEFAULTS } from '../system/transports/udp-multicast-transport/shared/UDPMulticastTypes';
import { generateUUID } from '../system/core/types/CrossPlatformUUID';

interface TestResult {
  testName: string;
  success: boolean;
  error?: string;
  details?: Record<string, unknown>;
}

/**
 * Test the UDP Transport constructor and basic properties
 */
async function testUDPTransportConstruction(): Promise<TestResult> {
  try {
    const config = {
      nodeId: generateUUID(),
      nodeType: NodeType.SERVER,
      capabilities: [NodeCapability.FILE_OPERATIONS],
      unicastPort: 45000 + Math.floor(Math.random() * 1000)
    };

    const transport = new UDPMulticastTransportServer(config);

    const details = {
      name: transport.name,
      hasIsConnected: typeof transport.isConnected === 'function',
      hasConnectedProperty: 'connected' in transport,
      hasInitialize: typeof (transport as any).initialize === 'function',
      hasOnMessage: typeof (transport as any).onMessage === 'function',
      hasSetMessageHandler: typeof transport.setMessageHandler === 'function',
      constructorWorked: true
    };

    return {
      testName: 'UDP Transport Construction',
      success: true,
      details
    };

  } catch (error: any) {
    return {
      testName: 'UDP Transport Construction',
      success: false,
      error: error.message
    };
  }
}

/**
 * Test transport initialization
 */
async function testUDPTransportInitialization(): Promise<TestResult> {
  try {
    const config = {
      nodeId: generateUUID(),
      nodeType: NodeType.SERVER,
      capabilities: [NodeCapability.FILE_OPERATIONS],
      unicastPort: 45001 + Math.floor(Math.random() * 1000)
    };

    const transport = new UDPMulticastTransportServer(config);

    // Test what methods are actually available
    const beforeInit = {
      isConnectedMethod: transport.isConnected(),
      connectedProperty: (transport as any).connected
    };

    // Try to initialize if method exists
    if (typeof (transport as any).initialize === 'function') {
      await (transport as any).initialize();
    }

    const afterInit = {
      isConnectedMethod: transport.isConnected(),
      connectedProperty: (transport as any).connected
    };

    const details = {
      beforeInit,
      afterInit,
      initializeMethodExists: typeof (transport as any).initialize === 'function'
    };

    // Clean up
    if (typeof transport.disconnect === 'function') {
      await transport.disconnect();
    }

    return {
      testName: 'UDP Transport Initialization',
      success: true,
      details
    };

  } catch (error: any) {
    return {
      testName: 'UDP Transport Initialization', 
      success: false,
      error: error.message
    };
  }
}

/**
 * Test message handling setup
 */
async function testMessageHandling(): Promise<TestResult> {
  try {
    const config = {
      nodeId: generateUUID(),
      nodeType: NodeType.SERVER,
      capabilities: [NodeCapability.FILE_OPERATIONS],
      unicastPort: 45002 + Math.floor(Math.random() * 1000)
    };

    const transport = new UDPMulticastTransportServer(config);

    let messageReceived = false;
    let receivedMessage: any = null;

    // Test different message handler patterns
    const handlerPatterns = {
      setMessageHandler: false,
      onMessage: false
    };

    // Try setMessageHandler
    if (typeof transport.setMessageHandler === 'function') {
      transport.setMessageHandler((message) => {
        messageReceived = true;
        receivedMessage = message;
      });
      handlerPatterns.setMessageHandler = true;
    }

    // Try onMessage pattern
    if (typeof (transport as any).onMessage === 'function') {
      (transport as any).onMessage((message: any) => {
        messageReceived = true;
        receivedMessage = message;
      });
      handlerPatterns.onMessage = true;
    }

    const details = {
      handlerPatterns,
      messageHandlerSet: messageReceived !== undefined
    };

    return {
      testName: 'Message Handling',
      success: Object.values(handlerPatterns).some(Boolean),
      details
    };

  } catch (error: any) {
    return {
      testName: 'Message Handling',
      success: false,
      error: error.message
    };
  }
}

/**
 * Test interface consistency
 */
async function testInterfaceConsistency(): Promise<TestResult> {
  try {
    const config = {
      nodeId: generateUUID(),
      nodeType: NodeType.SERVER,
      capabilities: [NodeCapability.FILE_OPERATIONS], 
      unicastPort: 45003 + Math.floor(Math.random() * 1000)
    };

    const transport = new UDPMulticastTransportServer(config);

    const interfaceCheck = {
      // JTAGTransport interface methods
      hasName: typeof transport.name === 'string',
      hasSend: typeof transport.send === 'function',
      hasIsConnected: typeof transport.isConnected === 'function',
      hasDisconnect: typeof transport.disconnect === 'function',
      hasReconnect: typeof transport.reconnect === 'function',
      hasSetMessageHandler: typeof transport.setMessageHandler === 'function',
      
      // Common patterns found in other transports
      hasConnectedProperty: 'connected' in transport,
      hasInitialize: typeof (transport as any).initialize === 'function',
      hasOnMessage: typeof (transport as any).onMessage === 'function',
      hasGetStats: typeof (transport as any).getStats === 'function',
      
      // Transport specific
      transportName: transport.name,
      constructorArgs: Object.keys(config)
    };

    return {
      testName: 'Interface Consistency',
      success: true,
      details: interfaceCheck
    };

  } catch (error: any) {
    return {
      testName: 'Interface Consistency',
      success: false,
      error: error.message
    };
  }
}

/**
 * Run all architecture discovery tests
 */
async function runArchitectureDiscovery(): Promise<void> {
  console.log('🔍 ARCHITECTURE DISCOVERY TESTS');
  console.log('=====================================');
  
  const tests = [
    testUDPTransportConstruction,
    testUDPTransportInitialization, 
    testMessageHandling,
    testInterfaceConsistency
  ];

  const results: TestResult[] = [];
  
  for (const testFn of tests) {
    console.log(`\\n🧪 Running: ${testFn.name}...`);
    const result = await testFn();
    
    const status = result.success ? '✅ PASSED' : '❌ FAILED';
    console.log(`   ${status}`);
    
    if (result.error) {
      console.log(`   Error: ${result.error}`);
    }
    
    if (result.details) {
      console.log(`   Details:`, JSON.stringify(result.details, null, 4));
    }
    
    results.push(result);
  }
  
  // Summary
  console.log('\\n📊 ARCHITECTURE DISCOVERY SUMMARY:');
  console.log('====================================');
  
  const passed = results.filter(r => r.success).length;
  const total = results.length;
  
  console.log(`Tests passed: ${passed}/${total}`);
  
  if (passed === total) {
    console.log('✅ All architecture discovery tests passed');
    console.log('🚀 Ready to build proper abstractions');
  } else {
    console.log('⚠️  Some architecture issues discovered');
    console.log('🔧 Fix these before building abstractions');
  }
}

// Run if executed directly
if (require.main === module) {
  runArchitectureDiscovery().catch(error => {
    console.error('❌ Architecture discovery failed:', error);
    process.exit(1);
  });
}

export { runArchitectureDiscovery };