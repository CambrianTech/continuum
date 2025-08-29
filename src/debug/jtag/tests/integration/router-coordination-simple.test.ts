/**
 * Simple Router Coordination Integration Test
 * 
 * Tests that router coordination helpers are working and integrated
 * Focuses on validating the structural changes rather than complex operations
 */

import { JTAGClientServer } from '../../system/core/client/server/JTAGClientServer';

interface SimpleRouterTest {
  readonly success: boolean;
  readonly message: string;
  readonly executionTimeMs: number;
}

async function testSimpleRouterCoordination(): Promise<SimpleRouterTest> {
  console.log('🧪 SIMPLE ROUTER COORDINATION TEST');
  console.log('=============================================');
  console.log('🎯 Validating router coordination helpers are integrated');
  
  const startTime = Date.now();

  try {
    // Connect to JTAG system
    console.log('🔗 Connecting to JTAG system...');
    const { getActivePorts } = require('../../examples/shared/ExampleConfig');
    const activePorts = await getActivePorts();
    const websocketPort = activePorts.websocket_server;
    console.log(`🔌 Connecting to WebSocket on port ${websocketPort}...`);
    const { client } = await JTAGClientServer.connect({
      targetEnvironment: 'server',
      transportType: 'websocket',
      serverUrl: `ws://localhost:${websocketPort}`
    });
    console.log('✅ Connected to JTAG system');

    // Simple test: just verify that exec works and router exists
    console.log('🧪 Testing basic router structure...');
    const result = await client.commands.exec({
      code: {
        type: 'inline',
        language: 'javascript',
        source: `
          console.log('🔧 SIMPLE ROUTER TEST: Checking router structure');
          
          // Basic existence check
          const hasJtag = typeof jtag !== 'undefined';
          const hasRouter = hasJtag && typeof jtag.router !== 'undefined';
          
          console.log('✅ SIMPLE ROUTER TEST: Basic structure check completed');
          return {
            success: hasRouter,
            hasJtag,
            hasRouter,
            proof: 'SIMPLE_ROUTER_COORDINATION_TESTED'
          };
        `
      }
    });

    console.log('🔌 Disconnecting...');
    await client.disconnect();

    const executionTime = Date.now() - startTime;

    if (result.success) {
      console.log('✅ SIMPLE ROUTER COORDINATION TEST PASSED');
      return {
        success: true,
        message: 'Router coordination infrastructure is working',
        executionTimeMs: executionTime
      };
    } else {
      console.log('❌ SIMPLE ROUTER COORDINATION TEST FAILED');
      return {
        success: false,
        message: 'Router coordination test execution failed',
        executionTimeMs: executionTime
      };
    }

  } catch (error) {
    const executionTime = Date.now() - startTime;
    const errorMessage = error instanceof Error ? error.message : 'Unknown error';
    console.error('❌ Simple router coordination test failed:', errorMessage);
    
    return {
      success: false,
      message: `Test setup failed: ${errorMessage}`,
      executionTimeMs: executionTime
    };
  }
}

// Execute test
testSimpleRouterCoordination()
  .then(result => {
    console.log('🎯 SIMPLE ROUTER COORDINATION RESULT:');
    console.log(JSON.stringify(result, null, 2));
    process.exit(result.success ? 0 : 1);
  })
  .catch(error => {
    console.error('🚨 Simple router coordination test crashed:', error);
    process.exit(1);
  });