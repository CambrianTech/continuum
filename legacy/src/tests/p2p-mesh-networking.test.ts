/**
 * P2P Mesh Networking Test
 * 
 * Tests the UDP multicast transport in its proper P2P mesh networking context.
 * Uses multi-node scenarios that create peer networks for realistic testing.
 */

import { 
  TransportTestSuite, 
  TransportTestRunner, 
  TestResultFormatter,
  TestEnvironment,
  TestCategory 
} from './framework/TransportTestFramework';
import { UDPTransportFactory } from './factories/UDPTransportFactory';
import { P2P_MULTI_NODE_SCENARIOS } from './scenarios/P2PMultiNodeScenarios';

/**
 * Run P2P mesh networking tests
 */
async function runP2PMeshNetworkingTests(): Promise<void> {
  console.log('🌐 P2P MESH NETWORKING TESTS');
  console.log('=============================');
  console.log('Testing UDP multicast transport in proper P2P mesh context');
  console.log('Creates multiple transport nodes that discover and communicate with each other');
  console.log();

  // Create factory and test runner (though P2P scenarios manage their own transports)
  const factory = new UDPTransportFactory();
  const runner = new TransportTestRunner(factory, TestEnvironment.SERVER);
  
  try {
    // Create P2P-specific test suite
    const testSuite = new TransportTestSuite('P2P Mesh Networking');
    testSuite.addScenarios(P2P_MULTI_NODE_SCENARIOS);
    
    console.log(`📋 Test Suite: ${testSuite.name}`);
    console.log(`🏭 Transport Factory: ${factory.name}`);
    console.log(`🌍 Test Environment: ${TestEnvironment.SERVER}`);
    console.log(`📂 Test Categories: ${testSuite.getCategories().join(', ')}`);
    console.log();
    
    // Run the P2P test suite
    const result = await testSuite.execute(runner);
    
    // Format and display results
    TestResultFormatter.formatSuiteResult(result);
    
    // Additional P2P-specific analysis
    console.log('\\n🔍 P2P MESH ANALYSIS:');
    console.log('======================');
    
    const p2pResults = result.results.filter(r => 
      r.category === TestCategory.DISCOVERY || r.category === TestCategory.MESSAGE_PASSING
    );
    
    if (p2pResults.length > 0) {
      const discoveryResults = p2pResults.filter(r => r.category === TestCategory.DISCOVERY);
      const messageResults = p2pResults.filter(r => r.category === TestCategory.MESSAGE_PASSING);
      
      if (discoveryResults.length > 0) {
        const discovery = discoveryResults[0];
        console.log(`📡 Peer Discovery: ${discovery.success ? '✅ WORKING' : '❌ FAILED'}`);
        if (discovery.metrics) {
          const metrics = discovery.metrics as any;
          console.log(`   Nodes Created: ${metrics.nodesCreated || 'unknown'}`);
          console.log(`   Peers Discovered: ${metrics.peersDiscovered || 'unknown'}`);
        }
      }
      
      if (messageResults.length > 0) {
        const messaging = messageResults[0];
        console.log(`📨 Message Exchange: ${messaging.success ? '✅ WORKING' : '❌ FAILED'}`);
        if (messaging.metrics) {
          const metrics = messaging.metrics as any;
          console.log(`   Messages Exchanged: ${metrics.messagesExchanged || 'unknown'}`);
          console.log(`   Cross-Peer Success: ${metrics.crossPeerSuccess ? '✅' : '❌'}`);
        }
      }
    }
    
    // Grid readiness assessment
    console.log('\\n🌟 THE GRID READINESS ASSESSMENT:');
    console.log('==================================');
    
    if (result.successRate === 100) {
      console.log('🎉 THE GRID IS READY!');
      console.log('✨ P2P mesh networking is fully functional');
      console.log('🚀 Ready to implement Flynn\'s distributed mesh vision');
    } else if (result.successRate >= 50) {
      console.log('🔧 THE GRID IS PARTIALLY READY');
      console.log('⚡ Some P2P functionality working, needs refinement');
      console.log('🛠️ Continue development with incremental improvements'); 
    } else {
      console.log('🚧 THE GRID NEEDS MORE WORK');
      console.log('💡 P2P mesh foundation needs strengthening');
      console.log('🔨 Focus on core transport reliability first');
    }
    
    // Exit with appropriate code
    const exitCode = result.successRate >= 50 ? 0 : 1; // 50% threshold for P2P systems
    console.log(`\\n🏁 Test execution complete - exit code: ${exitCode}`);
    
    process.exit(exitCode);
    
  } catch (error: any) {
    console.error('\\n💥 P2P test suite execution failed:', error.message);
    console.error('Stack trace:', error.stack);
    process.exit(1);
  } finally {
    // Ensure cleanup
    try {
      await runner.cleanup();
      await factory.cleanupAll();
    } catch (cleanupError: any) {
      console.warn('⚠️ Cleanup error:', cleanupError.message);
    }
  }
}

// Run if executed directly
if (require.main === module) {
  runP2PMeshNetworkingTests().catch(error => {
    console.error('❌ P2P mesh test suite crashed:', error);
    process.exit(1);
  });
}

export { runP2PMeshNetworkingTests };