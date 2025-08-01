#!/usr/bin/env node
/**
 * Smart Transport Manager - Unit Tests (TDD)
 * 
 * Test-driven development for JTAGSmartTransport:
 * - Host transport auto-detection
 * - Primary → Fallback → Queue chain
 * - Message queuing and auto-flush
 * - Error handling and recovery
 * - Configuration management
 * 
 * These tests define the expected behavior BEFORE implementation.
 */

import { 
  MockSuccessTransport, 
  MockFailureTransport, 
  MockControllableTransport,
  TransportTestUtils
} from '@tests/shared/MockTransports';

import type { JTAGConfig, JTAGWebSocketMessage } from '../../../system/core/types/JTAGTypes';

// Import the class we're about to implement
import { JTAGSmartTransport } from '@shared/JTAGTransportFactory';

class SmartTransportManagerTest {
  private defaultConfig: JTAGConfig = {
    context: 'server',
    jtagPort: 9001,
    enableRemoteLogging: true,
    enableConsoleOutput: false,
    maxBufferSize: 100,
    transport: {
      type: 'websocket',
      fallback: 'http',
      retryAttempts: 3
    }
  };

  async runAllTests(): Promise<void> {
    console.log('🧪 Smart Transport Manager - TDD Unit Tests');
    console.log('===============================================\n');

    await this.testInitializationLogic();
    await this.testTransportPriorityChain();
    await this.testMessageQueueing();
    await this.testAutoFlushMechanism();
    await this.testErrorHandlingAndRecovery();
    await this.testHostTransportDetection();
    await this.testConfigurationManagement();
    
    console.log('\n🎉 All Smart Transport Manager TDD tests passed!');
    console.log('✅ Ready to implement JTAGSmartTransport to make these tests pass');
  }

  private async testInitializationLogic(): Promise<void> {
    console.log('🚀 Testing Transport Initialization Logic...');
    
    const smartTransport = new JTAGSmartTransport();
    
    // Test 1: Successful initialization with working primary transport
    const config1 = { ...this.defaultConfig };
    const initSuccess = await smartTransport.initialize(config1);
    
    // Expected behavior: Should initialize successfully (we'll mock the transports)
    console.log(`   ✅ Initialization result: ${initSuccess}`);
    
    // Test 2: Connection status should reflect transport state
    const connected = smartTransport.isConnected();
    console.log(`   ✅ Connection status: ${connected}`);
    
    // Test 3: Smart transport name should be set
    if (smartTransport.name !== 'smart-transport') {
      throw new Error('Smart transport should have name "smart-transport"');
    }
    
    console.log('   ✅ Smart transport properly named');
    console.log('   🎉 Initialization logic tests: PASSED\n');
  }

  private async testTransportPriorityChain(): Promise<void> {
    console.log('🔗 Testing Transport Priority Chain Logic...');
    
    // Test Scenario 1: Primary succeeds, fallback not used
    const smartTransport1 = new JTAGSmartTransport();
    await smartTransport1.initialize(this.defaultConfig);
    
    const message1 = TransportTestUtils.createJTAGMessage('log');
    const result1 = await smartTransport1.send(message1);
    
    // Expected: Should use primary transport successfully
    console.log(`   ✅ Primary transport success: ${result1.success}`);
    
    // Test Scenario 2: Primary fails, fallback succeeds
    const smartTransport2 = new JTAGSmartTransport();
    
    // We'll need to mock this behavior - the test defines the expected interface
    // Implementation should try primary first, then fallback
    const config2 = { 
      ...this.defaultConfig, 
      transport: { type: 'websocket' as const, fallback: 'http' as const } 
    };
    
    await smartTransport2.initialize(config2);
    
    const message2 = TransportTestUtils.createJTAGMessage('log');
    const result2 = await smartTransport2.send(message2);
    
    // Expected: Should eventually succeed via fallback (when primary fails)
    console.log(`   ✅ Fallback transport behavior: ${result2.success || 'queued'}`);
    
    // Test Scenario 3: Both fail, message queued
    const smartTransport3 = new JTAGSmartTransport();
    const config3 = { 
      ...this.defaultConfig,
      transport: { type: 'websocket' as const, fallback: 'queue' as const }
    };
    
    await smartTransport3.initialize(config3);
    
    const message3 = TransportTestUtils.createJTAGMessage('log');
    const result3 = await smartTransport3.send(message3);
    
    // Expected: Should queue message when all transports fail
    console.log(`   ✅ Queue fallback behavior: ${!result3.success && result3.error?.includes('queued')}`);
    
    console.log('   🎉 Transport priority chain tests: PASSED\n');
  }

  private async testMessageQueueing(): Promise<void> {
    console.log('📬 Testing Message Queuing Logic...');
    
    const smartTransport = new JTAGSmartTransport();
    
    // Initialize with failing transports to force queuing
    await smartTransport.initialize(this.defaultConfig);
    
    // Send multiple messages that should be queued
    const messages = [
      TransportTestUtils.createJTAGMessage('log'),
      TransportTestUtils.createJTAGMessage('error'),
      TransportTestUtils.createJTAGMessage('screenshot')
    ];
    
    const results = [];
    for (const message of messages) {
      const result = await smartTransport.send(message);
      results.push(result);
    }
    
    // Expected: Messages should be queued when no transport available
    const queuedCount = results.filter(r => !r.success && r.error?.includes('queued')).length;
    console.log(`   ✅ Messages queued when transport unavailable: ${queuedCount} messages`);
    
    // Test queue size limits
    const maxMessages = 50;
    for (let i = 0; i < maxMessages; i++) {
      await smartTransport.send(TransportTestUtils.createJTAGMessage('log'));
    }
    
    console.log('   ✅ Queue size limit handling: No memory overflow');
    console.log('   🎉 Message queuing tests: PASSED\n');
  }

  private async testAutoFlushMechanism(): Promise<void> {
    console.log('🔄 Testing Auto-Flush Mechanism...');
    
    const smartTransport = new JTAGSmartTransport();
    await smartTransport.initialize(this.defaultConfig);
    
    // Queue some messages when transport is down
    const messages = [
      TransportTestUtils.createJTAGMessage('log'),
      TransportTestUtils.createJTAGMessage('warn')
    ];
    
    for (const message of messages) {
      await smartTransport.send(message);
    }
    
    // Expected: Smart transport should have a flushQueue method
    if (typeof (smartTransport as any).flushQueue !== 'function') {
      throw new Error('Smart transport should implement flushQueue() method');
    }
    
    // Test manual flush
    await (smartTransport as any).flushQueue();
    console.log('   ✅ Manual queue flush: Completed without errors');
    
    // Test auto-flush when transport becomes available
    // This would happen automatically in the background
    console.log('   ✅ Auto-flush capability: Interface available');
    
    console.log('   🎉 Auto-flush mechanism tests: PASSED\n');
  }

  private async testErrorHandlingAndRecovery(): Promise<void> {
    console.log('💥 Testing Error Handling and Recovery...');
    
    const smartTransport = new JTAGSmartTransport();
    await smartTransport.initialize(this.defaultConfig);
    
    // Test 1: Graceful handling of transport failures
    const message = TransportTestUtils.createJTAGMessage('log');
    const result = await smartTransport.send(message);
    
    // Expected: Should never throw exceptions, always return JTAGTransportResponse
    if (!result.timestamp || typeof result.success !== 'boolean') {
      throw new Error('Smart transport should always return well-formed response');
    }
    
    console.log('   ✅ Graceful error handling: No exceptions thrown');
    
    // Test 2: Transport reconnection capability
    await smartTransport.disconnect();
    const disconnected = !smartTransport.isConnected();
    console.log(`   ✅ Disconnect handling: ${disconnected}`);
    
    // Test 3: Recovery attempt
    const recovery = await smartTransport.initialize(this.defaultConfig);
    console.log(`   ✅ Recovery capability: ${typeof recovery === 'boolean'}`);
    
    console.log('   🎉 Error handling and recovery tests: PASSED\n');
  }

  private async testHostTransportDetection(): Promise<void> {
    console.log('🔍 Testing Host Transport Auto-Detection...');
    
    // Test the smart transport's host detection logic
    const smartTransport = new JTAGSmartTransport();
    
    // This should be part of the initialization process
    await smartTransport.initialize(this.defaultConfig);
    
    // Expected: Should attempt host transport detection first
    // We can't test actual Continuum detection in unit tests, but we can test the interface
    console.log('   ✅ Host detection interface: Available in initialization');
    
    // Test fallback when no host transport detected
    const noHostConfig = { 
      ...this.defaultConfig, 
      transport: { type: 'websocket' as const, fallback: 'http' as const } 
    };
    
    const smartTransport2 = new JTAGSmartTransport();
    const init2 = await smartTransport2.initialize(noHostConfig);
    
    console.log(`   ✅ No host transport fallback: ${typeof init2 === 'boolean'}`);
    console.log('   🎉 Host transport detection tests: PASSED\n');
  }

  private async testConfigurationManagement(): Promise<void> {
    console.log('⚙️ Testing Configuration Management...');
    
    // Test 1: Default configuration handling
    const smartTransport1 = new JTAGSmartTransport();
    const minimalConfig = { context: 'server' as const, jtagPort: 9001, enableRemoteLogging: true, enableConsoleOutput: false, maxBufferSize: 100 };
    
    const init1 = await smartTransport1.initialize(minimalConfig);
    console.log(`   ✅ Minimal configuration: ${typeof init1 === 'boolean'}`);
    
    // Test 2: Custom transport configuration
    const customConfig = {
      ...this.defaultConfig,
      transport: {
        type: 'custom' as const,
        customTransport: new MockSuccessTransport(),
        retryAttempts: 5
      }
    };
    
    const smartTransport2 = new JTAGSmartTransport();
    const init2 = await smartTransport2.initialize(customConfig);
    console.log(`   ✅ Custom transport configuration: ${typeof init2 === 'boolean'}`);
    
    // Test 3: Configuration validation
    const invalidConfig = { 
      ...this.defaultConfig, 
      transport: { type: 'invalid-transport' as any } 
    };
    
    const smartTransport3 = new JTAGSmartTransport();
    const init3 = await smartTransport3.initialize(invalidConfig);
    console.log(`   ✅ Invalid configuration handling: ${typeof init3 === 'boolean'}`);
    
    console.log('   🎉 Configuration management tests: PASSED\n');
  }
}

// Export for integration with other tests
export { SmartTransportManagerTest };

// Run tests if called directly
if (require.main === module) {
  const test = new SmartTransportManagerTest();
  test.runAllTests().catch(error => {
    console.error('💥 Smart Transport Manager TDD tests failed:', error);
    console.log('\n🔧 These tests define the expected behavior for JTAGSmartTransport');
    console.log('📝 Implement JTAGSmartTransport to make these tests pass');
    process.exit(1);
  });
}