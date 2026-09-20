/**
 * JTAG Client + Connection Broker Integration Test
 * 
 * Tests the integration between JTAGClient and Connection Broker
 * without running the full system to isolate any issues.
 */

import { JTAGClientServer } from './system/core/client/server/JTAGClientServer';
import { generateUUID } from './system/core/types/CrossPlatformUUID';
import type { JTAGContext } from './system/core/types/JTAGTypes';

async function testJTAGClientWithBroker() {
  console.log('🧪 Testing JTAGClient + Connection Broker integration...\n');
  
  const context: JTAGContext = {
    uuid: generateUUID(),
    environment: 'server'
  };
  
  try {
    console.log('📋 Test 1: Create JTAGClientServer with Connection Broker');
    
    const client = new JTAGClientServer(context);
    
    console.log('📋 Test 2: Connect using Connection Broker (this will test our integration)');
    
    // This should use the Connection Broker internally
    const result = await JTAGClientServer.connectRemote({
      targetEnvironment: 'server',
      sessionId: generateUUID(),
      timeout: 10000
    });
    
    console.log(`✅ Connection successful!`);
    console.log(`  - Connection type: ${result.client.getConnectionInfo().connectionType}`);
    console.log(`  - Session ID: ${result.client.sessionId}`);
    console.log(`  - Available commands: ${result.listResult.totalCount}`);
    
    console.log('\n📋 Test 3: Cleanup');
    await result.client.disconnect();
    console.log('✅ Client disconnected successfully');
    
    console.log('\n🎉 JTAG Client + Connection Broker integration test passed!');
    return true;
    
  } catch (error) {
    console.error('\n❌ Integration test failed:', error);
    console.error('Stack trace:', (error as Error).stack);
    return false;
  }
}

// Run the test
if (import.meta.url === `file://${process.argv[1]}`) {
  testJTAGClientWithBroker().then(success => {
    process.exit(success ? 0 : 1);
  }).catch(error => {
    console.error('💥 Test execution error:', error);
    process.exit(1);
  });
}