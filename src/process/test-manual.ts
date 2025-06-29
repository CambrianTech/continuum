/**
 * Manual Multi-Process System Test
 * Test the coordinator and version daemon without Jest
 */

import { ProcessCoordinator } from './coordinator/ProcessCoordinator.js';

async function runManualTest() {
  console.log('🧪 Running manual ProcessCoordinator test...');
  
  const coordinator = new ProcessCoordinator(['src/process/daemons']);
  
  try {
    await coordinator.start();
    
    console.log('📦 Available daemons:', coordinator.getAvailable());
    
    const processId = await coordinator.spawn('version');
    console.log('🚀 Spawned process:', processId);
    
    // Test version request
    const versionResult = await coordinator.route({
      id: 'manual-test-version',
      type: 'version',
      data: {},
      timestamp: Date.now()
    });
    console.log('📋 Version result:', versionResult);
    
    // Test info request
    const infoResult = await coordinator.route({
      id: 'manual-test-info',
      type: 'info',
      data: {},
      timestamp: Date.now()
    });
    console.log('ℹ️ Info result:', infoResult);
    
    // Test health check
    const health = await coordinator.healthCheck();
    console.log('🏥 Health check:', health);
    
    // Test system status
    const status = coordinator.getSystemStatus();
    console.log('📊 System status:', status);
    
    console.log('✅ Manual test completed successfully');
    
  } catch (error) {
    console.error('❌ Manual test failed:', error);
    throw error;
  } finally {
    await coordinator.stop();
  }
}

// Run the test
runManualTest().catch(error => {
  console.error('💥 Test execution failed:', error);
  process.exit(1);
});