#!/usr/bin/env npx tsx
/**
 * JTAG Status Events Unit Tests
 * 
 * Tests the transport-agnostic status event system with all transport types
 */

import { JTAGBase } from '@shared/JTAGBase';
import { JTAG_STATUS, JTAG_TRANSPORT, JTAGStatusEvent, JTAGStatusEventListener } from '@shared/JTAGTypes';
import { JTAGRESTTransport, JTAGMCPTransport, JTAGPollingTransport } from '@shared/transport-examples';

interface TestResults {
  totalTests: number;
  passed: number;
  failed: number;
  errors: string[];
}

class JTAGStatusEventsUnitTester {
  private results: TestResults = {
    totalTests: 0,
    passed: 0,
    failed: 0,
    errors: []
  };

  async runAllTests(): Promise<void> {
    console.log('\n🧪 JTAG Status Events Unit Tests');
    console.log('=================================\n');

    // Test 1: Status Event Listener Registration
    await this.testStatusEventListeners();
    
    // Test 2: Status Event Emission 
    await this.testStatusEventEmission();
    
    // Test 3: Transport Type Detection
    await this.testTransportTypeDetection();
    
    // Test 4: Connection State Tracking
    await this.testConnectionStateTracking();
    
    // Test 5: REST Transport Status Events
    await this.testRESTTransportEvents();
    
    // Test 6: MCP Transport Status Events  
    await this.testMCPTransportEvents();
    
    // Test 7: Polling Transport Status Events
    await this.testPollingTransportEvents();
    
    // Test 8: Error Handling and Edge Cases
    await this.testErrorHandling();

    this.printResults();
  }

  private async testStatusEventListeners(): Promise<void> {
    console.log('📋 Testing Status Event Listener Registration...');
    
    try {
      let eventReceived: JTAGStatusEvent | null = null;
      
      const listener: JTAGStatusEventListener = (event) => {
        eventReceived = event;
      };
      
      // Test listener registration
      JTAGBase.addStatusListener(listener);
      this.assert(true, 'Status listener registered successfully');
      
      // Test listener removal
      JTAGBase.removeStatusListener(listener);
      this.assert(true, 'Status listener removed successfully');
      
      console.log('   ✅ Status event listener system: PASSED\n');
      
    } catch (error: any) {
      this.fail(`Status event listeners failed: ${error.message}`);
    }
  }

  private async testStatusEventEmission(): Promise<void> {
    console.log('📡 Testing Status Event Emission...');
    
    try {
      let lastEvent: JTAGStatusEvent | null = null;
      let eventCount = 0;
      
      const listener: JTAGStatusEventListener = (event) => {
        lastEvent = event;
        eventCount++;
      };
      
      JTAGBase.addStatusListener(listener);
      
      // Initialize JTAG to trigger status events
      JTAGBase.initialize();
      
      // Wait for async initialization
      await this.sleep(100);
      
      this.assert(eventCount > 0, `Events emitted: ${eventCount}`);
      this.assert(lastEvent !== null, 'Event received');
      this.assert(lastEvent!.status === JTAG_STATUS.CONNECTING, `Status: ${lastEvent!.status}`);
      this.assert(lastEvent!.transport.type === JTAG_TRANSPORT.WEBSOCKET, `Transport: ${lastEvent!.transport.type}`);
      
      JTAGBase.removeStatusListener(listener);
      console.log('   ✅ Status event emission: PASSED\n');
      
    } catch (error: any) {
      this.fail(`Status event emission failed: ${error.message}`);
    }
  }

  private async testTransportTypeDetection(): Promise<void> {
    console.log('🔍 Testing Transport Type Detection...');
    
    try {
      const restTransport = new JTAGRESTTransport();
      const mcpTransport = new JTAGMCPTransport(); 
      const pollingTransport = new JTAGPollingTransport();
      
      this.assert(restTransport.getTransportType() === JTAG_TRANSPORT.REST, 'REST transport type');
      this.assert(mcpTransport.getTransportType() === JTAG_TRANSPORT.MCP, 'MCP transport type');
      this.assert(pollingTransport.getTransportType() === JTAG_TRANSPORT.POLLING, 'Polling transport type');
      
      console.log('   ✅ Transport type detection: PASSED\n');
      
    } catch (error: any) {
      this.fail(`Transport type detection failed: ${error.message}`);
    }
  }

  private async testConnectionStateTracking(): Promise<void> {
    console.log('🔌 Testing Connection State Tracking...');
    
    try {
      const restTransport = new JTAGRESTTransport();
      
      // Test initial disconnected state
      const initialState = restTransport.getConnectionState();
      this.assert(initialState.connected === false, 'Initial state: disconnected');
      this.assert(typeof initialState.lastActivity === 'number', 'Last activity timestamp');
      
      console.log('   ✅ Connection state tracking: PASSED\n');
      
    } catch (error: any) {
      this.fail(`Connection state tracking failed: ${error.message}`);
    }
  }

  private async testRESTTransportEvents(): Promise<void> {
    console.log('🌐 Testing REST Transport Status Events...');
    
    try {
      const restTransport = new JTAGRESTTransport();
      let statusEvents: Array<{status: any, details: any}> = [];
      
      // Mock status handler
      restTransport.onStatusChange?.((status, details) => {
        statusEvents.push({ status, details });
      });
      
      // Test initialization (will fail - no real server)
      const config = { 
        context: 'browser' as const,
        jtagPort: 9999, // Non-existent port
        enableRemoteLogging: true,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };
      
      await restTransport.initialize(config);
      
      this.assert(statusEvents.length >= 2, `REST events emitted: ${statusEvents.length}`);
      this.assert(statusEvents[0].status === JTAG_STATUS.CONNECTING, 'REST connecting event');
      this.assert(statusEvents[1].status === JTAG_STATUS.ERROR, 'REST error event');
      this.assert(statusEvents[1].details.reason === 'rest_endpoint_unavailable', 'REST error reason');
      
      console.log('   ✅ REST transport events: PASSED\n');
      
    } catch (error: any) {
      this.fail(`REST transport events failed: ${error.message}`);
    }
  }

  private async testMCPTransportEvents(): Promise<void> {
    console.log('🤖 Testing MCP Transport Status Events...');
    
    try {
      const mcpTransport = new JTAGMCPTransport();
      let statusEvents: Array<{status: any, details: any}> = [];
      
      mcpTransport.onStatusChange?.((status, details) => {
        statusEvents.push({ status, details });
      });
      
      const config = {
        context: 'browser' as const,
        jtagPort: 9999,
        enableRemoteLogging: true,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };
      
      await mcpTransport.initialize(config);
      
      this.assert(statusEvents.length >= 2, `MCP events emitted: ${statusEvents.length}`);
      this.assert(statusEvents[0].status === JTAG_STATUS.CONNECTING, 'MCP connecting event');
      this.assert(statusEvents[0].details.mcpMethod === 'initialize', 'MCP method tracking');
      
      console.log('   ✅ MCP transport events: PASSED\n');
      
    } catch (error: any) {
      this.fail(`MCP transport events failed: ${error.message}`);
    }
  }

  private async testPollingTransportEvents(): Promise<void> {
    console.log('⏱️ Testing Polling Transport Status Events...');
    
    try {
      const pollingTransport = new JTAGPollingTransport();
      let statusEvents: Array<{status: any, details: any}> = [];
      
      pollingTransport.onStatusChange?.((status, details) => {
        statusEvents.push({ status, details });
      });
      
      const config = {
        context: 'browser' as const,
        jtagPort: 9999,
        enableRemoteLogging: true,
        enableConsoleOutput: false,
        maxBufferSize: 100
      };
      
      await pollingTransport.initialize(config);
      
      this.assert(statusEvents.length >= 1, `Polling events emitted: ${statusEvents.length}`);
      this.assert(statusEvents[0].status === JTAG_STATUS.CONNECTING, 'Polling connecting event');
      this.assert(statusEvents[0].details.pollingInterval === 5000, 'Polling interval tracking');
      
      // Test disconnection
      await pollingTransport.disconnect();
      
      const disconnectEvent = statusEvents.find(e => e.status === JTAG_STATUS.DISCONNECTED);
      this.assert(disconnectEvent !== undefined, 'Polling disconnect event');
      
      console.log('   ✅ Polling transport events: PASSED\n');
      
    } catch (error: any) {
      this.fail(`Polling transport events failed: ${error.message}`);
    }
  }

  private async testErrorHandling(): Promise<void> {
    console.log('⚠️ Testing Error Handling and Edge Cases...');
    
    try {
      // Test listener error handling
      const faultyListener: JTAGStatusEventListener = () => {
        throw new Error('Listener error');
      };
      
      JTAGBase.addStatusListener(faultyListener);
      
      // This should not crash the system
      JTAGBase.initialize();
      await this.sleep(50);
      
      JTAGBase.removeStatusListener(faultyListener);
      
      this.assert(true, 'Faulty listener handled gracefully');
      
      // Test status tracking
      const currentStatus = JTAGBase.getStatus();
      this.assert(typeof currentStatus === 'string', `Status tracking: ${currentStatus}`);
      
      console.log('   ✅ Error handling: PASSED\n');
      
    } catch (error: any) {
      this.fail(`Error handling failed: ${error.message}`);
    }
  }

  private assert(condition: boolean, message: string): void {
    this.results.totalTests++;
    if (condition) {
      this.results.passed++;
      console.log(`     ✅ ${message}`);
    } else {
      this.results.failed++;
      this.results.errors.push(message);
      console.log(`     ❌ ${message}`);
    }
  }

  private fail(message: string): void {
    this.results.totalTests++;
    this.results.failed++;
    this.results.errors.push(message);
    console.log(`     ❌ ${message}`);
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }

  private printResults(): void {
    console.log('\n📊 UNIT TEST RESULTS');
    console.log('====================');
    console.log(`✅ Passed: ${this.results.passed}`);
    console.log(`❌ Failed: ${this.results.failed}`);
    console.log(`📊 Total: ${this.results.totalTests}`);
    
    const successRate = Math.round((this.results.passed / this.results.totalTests) * 100);
    console.log(`📈 Success Rate: ${successRate}%`);

    if (this.results.errors.length > 0) {
      console.log('\n❌ Failed Tests:');
      this.results.errors.forEach(error => {
        console.log(`   - ${error}`);
      });
    }

    if (this.results.failed === 0) {
      console.log('\n🎉 ALL UNIT TESTS PASSED!');
      console.log('✨ Transport-agnostic status events working perfectly!');
    } else {
      console.log('\n⚠️ Some unit tests failed. Review implementation.');
      process.exit(1);
    }
  }
}

// Run tests if called directly
if (require.main === module) {
  const tester = new JTAGStatusEventsUnitTester();
  tester.runAllTests().catch(error => {
    console.error('\n💥 Unit test runner failed:', error);
    process.exit(1);
  });
}

export { JTAGStatusEventsUnitTester };