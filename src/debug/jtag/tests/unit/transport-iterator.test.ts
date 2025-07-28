#!/usr/bin/env npx tsx
/**
 * Transport Iterator Tests
 * Abstract tests that work with ALL transport implementations via iterator pattern
 */

import { JTAGTransportFactory } from '@shared/transports/TransportFactory';
import { BaseJTAGTransport } from '@shared/transports/BaseTransport';
import { JTAG_STATUS, JTAGConfig } from '@shared/JTAGTypes';

interface TestResults {
  totalTransports: number;
  totalTests: number;
  passed: number;
  failed: number;
  transportResults: Map<string, {passed: number, failed: number, errors: string[]}>;
}

class TransportIteratorTester {
  private results: TestResults = {
    totalTransports: 0,
    totalTests: 0,
    passed: 0,
    failed: 0,
    transportResults: new Map()
  };

  async runAllTransportTests(): Promise<void> {
    console.log('\n🔄 JTAG Transport Iterator Tests');
    console.log('==================================\n');

    // Test 1: Status Event Lifecycle for All Transports
    await this.testStatusEventLifecycle();
    
    // Test 2: Connection State Tracking for All Transports
    await this.testConnectionStateTracking();
    
    // Test 3: Error Handling for All Transports
    await this.testErrorHandling();
    
    // Test 4: Message Sending for All Transports
    await this.testMessageSending();

    this.printResults();
  }

  private async testStatusEventLifecycle(): Promise<void> {
    console.log('📡 Testing Status Event Lifecycle (All Transports)...\n');
    
    const results = await JTAGTransportFactory.runTransportTest(
      async (transport, definition) => {
        console.log(`  🔧 Testing: ${definition.name}...`);
        
        // Enable test mode
        transport.enableTestMode();
        transport.clearStatusEvents();
        
        const config: JTAGConfig = {
          context: 'browser',
          jtagPort: 9999, // Non-existent port for testing
          enableRemoteLogging: true,
          enableConsoleOutput: false,
          maxBufferSize: 100
        };

        // Test initialization
        await transport.initialize(config);
        
        // Check for CONNECTING status
        const hasConnecting = transport.hasStatus(JTAG_STATUS.CONNECTING);
        if (!hasConnecting) {
          throw new Error('CONNECTING status not emitted');
        }
        
        // Should have either READY or ERROR status
        const hasReady = transport.hasStatus(JTAG_STATUS.READY);
        const hasError = transport.hasStatus(JTAG_STATUS.ERROR);
        
        if (!hasReady && !hasError) {
          throw new Error('Neither READY nor ERROR status emitted');
        }
        
        // Test disconnection
        await transport.disconnect();
        
        const hasDisconnected = transport.hasStatus(JTAG_STATUS.DISCONNECTED);
        if (!hasDisconnected) {
          throw new Error('DISCONNECTED status not emitted');
        }
        
        const events = transport.getStatusEvents();
        console.log(`    ✅ ${definition.name}: ${events.length} status events`);
        
        return {
          eventsEmitted: events.length,
          hasConnecting,
          hasReady,
          hasError,
          hasDisconnected
        };
      }
    );

    this.updateResults('Status Event Lifecycle', results);
  }

  private async testConnectionStateTracking(): Promise<void> {
    console.log('\n🔌 Testing Connection State Tracking (All Transports)...\n');
    
    const results = await JTAGTransportFactory.runTransportTest(
      async (transport, definition) => {
        console.log(`  🔧 Testing: ${definition.name}...`);
        
        transport.enableTestMode();
        
        // Test initial state
        const initialState = transport.getConnectionState();
        if (typeof initialState.connected !== 'boolean') {
          throw new Error('Connection state missing connected property');
        }
        
        if (!initialState.connectionId) {
          throw new Error('Connection state missing connectionId');
        }
        
        if (typeof initialState.lastActivity !== 'number') {
          throw new Error('Connection state missing lastActivity timestamp');
        }
        
        console.log(`    ✅ ${definition.name}: Connection state valid`);
        
        return {
          hasConnectionId: !!initialState.connectionId,
          hasLastActivity: typeof initialState.lastActivity === 'number',
          transportType: transport.getTransportType()
        };
      }
    );

    this.updateResults('Connection State Tracking', results);
  }

  private async testErrorHandling(): Promise<void> {
    console.log('\n⚠️ Testing Error Handling (All Transports)...\n');
    
    const results = await JTAGTransportFactory.runTransportTest(
      async (transport, definition) => {
        console.log(`  🔧 Testing: ${definition.name}...`);
        
        transport.enableTestMode();
        
        // Test graceful error handling with invalid config
        const invalidConfig: JTAGConfig = {
          context: 'browser',
          jtagPort: -1, // Invalid port
          enableRemoteLogging: true,
          enableConsoleOutput: false,
          maxBufferSize: 100
        };

        let errorCaught = false;
        try {
          await transport.initialize(invalidConfig);
        } catch (error) {
          errorCaught = true;
        }
        
        // Should either throw or emit ERROR status
        const hasErrorStatus = transport.hasStatus(JTAG_STATUS.ERROR);
        
        if (!errorCaught && !hasErrorStatus) {
          throw new Error('No error handling detected');
        }
        
        console.log(`    ✅ ${definition.name}: Error handling works`);
        
        return {
          errorCaught,
          hasErrorStatus,
          gracefulHandling: true
        };
      }
    );

    this.updateResults('Error Handling', results);
  }

  private async testMessageSending(): Promise<void> {
    console.log('\n📤 Testing Message Sending (All Transports)...\n');
    
    const results = await JTAGTransportFactory.runTransportTest(
      async (transport, definition) => {
        console.log(`  🔧 Testing: ${definition.name}...`);
        
        transport.enableTestMode();
        
        const testMessage = {
          type: 'log' as const,
          payload: { test: true },
          timestamp: new Date().toISOString(),
          messageId: 'test-message-' + Date.now()
        };
        
        // Test sending while disconnected
        const disconnectedResponse = await transport.send(testMessage);
        
        if (disconnectedResponse.success) {
          throw new Error('Send should fail when disconnected');
        }
        
        if (!disconnectedResponse.error) {
          throw new Error('Error message should be provided when send fails');
        }
        
        console.log(`    ✅ ${definition.name}: Message sending behavior correct`);
        
        return {
          failsWhenDisconnected: !disconnectedResponse.success,
          providesErrorMessage: !!disconnectedResponse.error,
          hasTransportMeta: !!disconnectedResponse.transportMeta
        };
      }
    );

    this.updateResults('Message Sending', results);
  }

  private updateResults(testName: string, results: Map<string, {result: any, error?: string}>): void {
    console.log(`\n📊 ${testName} Results:`);
    
    for (const [transportType, outcome] of results.entries()) {
      if (!this.results.transportResults.has(transportType)) {
        this.results.transportResults.set(transportType, {passed: 0, failed: 0, errors: []});
      }
      
      const transportResult = this.results.transportResults.get(transportType)!;
      
      if (outcome.error) {
        transportResult.failed++;
        transportResult.errors.push(`${testName}: ${outcome.error}`);
        this.results.failed++;
        console.log(`  ❌ ${transportType}: ${outcome.error}`);
      } else {
        transportResult.passed++;
        this.results.passed++;
        console.log(`  ✅ ${transportType}: PASSED`);
      }
      
      this.results.totalTests++;
    }
    
    this.results.totalTransports = results.size;
  }

  private printResults(): void {
    console.log('\n📊 TRANSPORT ITERATOR TEST RESULTS');
    console.log('===================================');
    console.log(`🚀 Transports Tested: ${this.results.totalTransports}`);
    console.log(`📋 Total Tests: ${this.results.totalTests}`);
    console.log(`✅ Passed: ${this.results.passed}`);
    console.log(`❌ Failed: ${this.results.failed}`);
    
    const successRate = Math.round((this.results.passed / this.results.totalTests) * 100);
    console.log(`📈 Success Rate: ${successRate}%\n`);

    // Per-transport breakdown
    console.log('🔍 Per-Transport Results:');
    for (const [transportType, result] of this.results.transportResults.entries()) {
      const total = result.passed + result.failed;
      const rate = Math.round((result.passed / total) * 100);
      console.log(`  ${transportType}: ${result.passed}/${total} (${rate}%)`);
      
      if (result.errors.length > 0) {
        result.errors.forEach(error => {
          console.log(`    ❌ ${error}`);
        });
      }
    }

    // List all tested transport types
    console.log('\n🔄 Transport Types Tested:');
    const testableTransports = JTAGTransportFactory.getTestableTransports();
    testableTransports.forEach(type => {
      const definition = JTAGTransportFactory.getTransportDefinition(type);
      console.log(`  ✅ ${type}: ${definition?.description}`);
    });

    if (this.results.failed === 0) {
      console.log('\n🎉 ALL TRANSPORT TESTS PASSED!');
      console.log('✨ All transport types implement the interface correctly!');
    } else {
      console.log('\n⚠️ Some transport tests failed. Review implementations.');
      process.exit(1);
    }
  }
}

// Run tests if called directly
if (require.main === module) {
  const tester = new TransportIteratorTester();
  tester.runAllTransportTests().catch(error => {
    console.error('\n💥 Transport iterator test runner failed:', error);
    process.exit(1);
  });
}

export { TransportIteratorTester };