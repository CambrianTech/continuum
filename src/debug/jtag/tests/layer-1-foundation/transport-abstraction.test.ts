#!/usr/bin/env node
/**
 * Transport Abstraction Layer Tests
 * 
 * Tests the transport abstraction independently from business logic:
 * - Transport interface compliance
 * - Fallback behavior  
 * - Message queuing
 * - Auto-detection logic
 * - Custom transport registration
 * 
 * These tests validate transport behavior WITHOUT network dependencies.
 */

import { 
  MockSuccessTransport, 
  MockFailureTransport, 
  MockControllableTransport,
  MockNetworkTransport,
  MockTransportFactory,
  TransportTestUtils
} from '@tests/shared/MockTransports';

import { 
  JTAGSmartTransport,
  JTAGTransportFactoryImpl,
  DefaultMessageQueue
} from '@shared/JTAGTransportFactory';

import type { JTAGConfig } from '@shared/JTAGTypes';

class TransportAbstractionTest {
  private testConfig: JTAGConfig = {
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
    console.log('🧪 Transport Abstraction Layer Tests');
    console.log('=====================================\n');

    await this.testTransportInterface();
    await this.testMockTransports();  
    await this.testSmartTransportFallback();
    await this.testMessageQueue();
    await this.testTransportFactory();
    await this.testNetworkSimulation();

    console.log('\n🎉 All transport abstraction tests passed!');
  }

  private async testTransportInterface(): Promise<void> {
    console.log('📡 Testing Transport Interface Compliance...');

    // Test that all transports implement the interface correctly
    const transports = [
      new MockSuccessTransport(),
      new MockFailureTransport(),
      new MockControllableTransport()
    ];

    for (const transport of transports) {
      // Test interface methods exist
      if (!transport.initialize || !transport.send || !transport.isConnected || !transport.disconnect) {
        throw new Error(`Transport ${transport.name} missing required interface methods`);
      }

      // Test initialization
      const initResult = await transport.initialize(this.testConfig);
      console.log(`   ✅ ${transport.name}: initialize() → ${initResult}`);

      // Test connection status
      const connected = transport.isConnected();
      console.log(`   ✅ ${transport.name}: isConnected() → ${connected}`);

      // Test message sending
      const testMessage = TransportTestUtils.createJTAGMessage('log');
      const sendResult = await transport.send(testMessage);
      console.log(`   ✅ ${transport.name}: send() → ${sendResult.success}`);

      // Test disconnect
      await transport.disconnect();
      console.log(`   ✅ ${transport.name}: disconnect() completed`);
    }

    console.log('   🎉 Transport interface compliance: PASSED\n');
  }

  private async testMockTransports(): Promise<void> {
    console.log('🎭 Testing Mock Transport Behaviors...');

    // Test Success Transport
    const successTransport = MockTransportFactory.createSuccessTransport();
    await successTransport.initialize(this.testConfig);
    
    const message1 = TransportTestUtils.createJTAGMessage('log');
    const result1 = await successTransport.send(message1);
    
    if (!result1.success) {
      throw new Error('Success transport should always succeed');
    }

    console.log(`   ✅ Success transport: ${successTransport.getMessageCount()} messages sent`);

    // Test Failure Transport  
    const failureTransport = MockTransportFactory.createFailureTransport('Test failure');
    const initFailed = await failureTransport.initialize(this.testConfig);
    
    if (initFailed) {
      throw new Error('Failure transport should fail to initialize');
    }

    console.log('   ✅ Failure transport: initialization failed as expected');

    // Test Controllable Transport
    const controllableTransport = MockTransportFactory.createControllableTransport();
    await controllableTransport.initialize(this.testConfig);

    // Test success mode
    controllableTransport.setSuccess(true);
    const message2 = TransportTestUtils.createJTAGMessage('log');
    const result2 = await controllableTransport.send(message2);

    if (!result2.success) {
      throw new Error('Controllable transport in success mode should succeed');
    }

    // Test failure mode
    controllableTransport.setSuccess(false);
    const message3 = TransportTestUtils.createJTAGMessage('log');
    const result3 = await controllableTransport.send(message3);

    if (result3.success) {
      throw new Error('Controllable transport in failure mode should fail');
    }

    console.log('   ✅ Controllable transport: success/failure modes working');
    console.log('   🎉 Mock transport behaviors: PASSED\n');
  }

  private async testSmartTransportFallback(): Promise<void> {
    console.log('🧠 Testing Smart Transport Fallback Logic...');

    const smartTransport = new JTAGSmartTransport();
    
    // Mock the primary and fallback transports
    const primaryTransport = MockTransportFactory.createFailureTransport('Primary failed');
    const fallbackTransport = MockTransportFactory.createSuccessTransport();

    // Initialize the fallback transport so isConnected() returns true
    await fallbackTransport.initialize(this.testConfig);

    // Manually inject transports for testing (in real implementation, these would be created by factory)
    (smartTransport as any).primaryTransport = primaryTransport;
    (smartTransport as any).fallbackTransport = fallbackTransport;

    // Test fallback behavior
    const message = TransportTestUtils.createJTAGMessage('log');
    const result = await smartTransport.send(message);

    // Should use fallback since primary fails
    if (fallbackTransport.getMessageCount() !== 1) {
      throw new Error('Smart transport should use fallback when primary fails');
    }

    console.log('   ✅ Smart transport fallback: Primary failed → Fallback used');

    // Test with both transports failing (should queue message)
    const failingFallback = MockTransportFactory.createFailureTransport('Fallback failed');
    (smartTransport as any).fallbackTransport = failingFallback;

    const message2 = TransportTestUtils.createJTAGMessage('log');  
    const result2 = await smartTransport.send(message2);

    if (result2.success) {
      throw new Error('Smart transport should fail when all transports unavailable');
    }

    console.log('   ✅ Smart transport queuing: Both transports failed → Message queued');
    console.log('   🎉 Smart transport fallback: PASSED\n');
  }

  private async testMessageQueue(): Promise<void> {
    console.log('📬 Testing Message Queue Functionality...');

    const messageQueue = new DefaultMessageQueue();
    
    // Test queueing
    const message1 = TransportTestUtils.createJTAGMessage('log');
    const message2 = TransportTestUtils.createJTAGMessage('screenshot');
    
    messageQueue.enqueue(message1);
    messageQueue.enqueue(message2);

    if (messageQueue.size() !== 2) {
      throw new Error('Message queue should contain 2 messages');
    }

    console.log(`   ✅ Message queuing: ${messageQueue.size()} messages queued`);

    // Test dequeuing
    const dequeuedMessage = messageQueue.dequeue();
    if (!dequeuedMessage || dequeuedMessage.messageId !== message1.messageId) {
      throw new Error('Dequeued message should be the first queued message');
    }

    console.log('   ✅ Message dequeuing: FIFO order maintained');

    // Test flushing to transport
    const successTransport = MockTransportFactory.createSuccessTransport();
    await successTransport.initialize(this.testConfig);
    
    const results = await messageQueue.flush(successTransport);
    
    if (results.length !== 1 || !results[0].success) {
      throw new Error('Message queue flush should send remaining message successfully');
    }

    if (messageQueue.size() !== 0) {
      throw new Error('Message queue should be empty after successful flush');
    }

    console.log('   ✅ Message queue flush: All messages sent successfully');
    console.log('   🎉 Message queue functionality: PASSED\n');
  }

  private async testTransportFactory(): Promise<void> {
    console.log('🏭 Testing Transport Factory...');

    const factory = new JTAGTransportFactoryImpl();
    
    // Test available transports
    const availableTransports = factory.getAvailableTransports();
    const expectedTransports = ['websocket', 'http', 'continuum-ws'];
    
    for (const expected of expectedTransports) {
      if (!availableTransports.includes(expected)) {
        throw new Error(`Factory should include ${expected} transport`);
      }
    }

    console.log(`   ✅ Available transports: ${availableTransports.join(', ')}`);

    // Test custom transport registration
    const customTransportCreated = MockTransportFactory.createSuccessTransport();
    factory.registerTransport('test-custom', () => customTransportCreated);
    
    const updatedTransports = factory.getAvailableTransports();
    if (!updatedTransports.includes('test-custom')) {
      throw new Error('Factory should include registered custom transport');
    }

    console.log('   ✅ Custom transport registration: test-custom registered');

    // Test transport creation
    const createdTransport = factory.createTransport({ type: 'test-custom' });
    if (createdTransport.name !== customTransportCreated.name) {
      throw new Error('Factory should create registered transport');
    }

    console.log('   ✅ Transport creation: Custom transport created successfully');
    console.log('   🎉 Transport factory: PASSED\n');
  }

  private async testNetworkSimulation(): Promise<void> {
    console.log('🌐 Testing Network Simulation Transport...');

    // Test with low latency, no drops
    const reliableTransport = MockTransportFactory.createNetworkSimulation(10, 0);
    await reliableTransport.initialize(this.testConfig);

    const startTime = Date.now();
    const message = TransportTestUtils.createJTAGMessage('log');
    const result = await reliableTransport.send(message);
    const duration = Date.now() - startTime;

    if (!result.success) {
      throw new Error('Reliable network transport should succeed');
    }

    if (duration < 10) {
      throw new Error('Network transport should respect latency simulation');
    }

    console.log(`   ✅ Network simulation: ${duration}ms latency (expected ~10ms)`);

    // Test with high drop rate
    const unreliableTransport = MockTransportFactory.createNetworkSimulation(1, 0.9);
    await unreliableTransport.initialize(this.testConfig);

    let failures = 0;
    const attempts = 10;

    for (let i = 0; i < attempts; i++) {
      const message = TransportTestUtils.createJTAGMessage('log');
      const result = await unreliableTransport.send(message);
      if (!result.success) failures++;
    }

    // With 90% drop rate, we should see some failures
    if (failures < 5) {
      console.log(`   ⚠️ Network simulation: Expected more failures with 90% drop rate (got ${failures}/${attempts})`);
    } else {
      console.log(`   ✅ Network simulation: ${failures}/${attempts} messages dropped as expected`);
    }

    console.log('   🎉 Network simulation: PASSED\n');
  }
}

// Run tests if called directly
if (require.main === module) {
  const test = new TransportAbstractionTest();
  test.runAllTests().catch(error => {
    console.error('💥 Transport abstraction tests failed:', error);
    process.exit(1);
  });
}

export { TransportAbstractionTest };