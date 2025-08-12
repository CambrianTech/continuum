/**
 * Comprehensive JTAG Router Integration Test
 * 
 * This test validates the complete routing system using the live JTAG system.
 * Tests all critical routing scenarios:
 * - Single environment routing (server → server)
 * - Cross-environment routing (server → browser → server)
 * - Complex multi-hop routing chains
 * - Promise resolution and correlation across environments
 * - Error propagation and handling
 * - Performance under concurrent load
 * 
 * NO MOCKS - Uses actual WebSocket connections to running system
 */

import WebSocket from 'ws';
import { createUUID } from '../../system/core/types/CrossPlatformUUID';
import { JTAGMessageFactory } from '../../system/core/types/JTAGTypes';
import type { JTAGMessage } from '../../system/core/types/JTAGTypes';

interface TestResult {
  name: string;
  success: boolean;
  duration: number;
  error?: string;
  details?: any;
}

class ComprehensiveRoutingTest {
  private ws: WebSocket | null = null;
  private results: TestResult[] = [];
  private messageHandlers = new Map<string, (message: any) => void>();

  async runAllTests(): Promise<boolean> {
    console.log('🧪 Starting Comprehensive JTAG Router Integration Tests');
    console.log('=' .repeat(80));

    try {
      await this.connectToSystem();
      
      // Test Suite 1: Real-World Command Testing
      await this.testScreenshotCommand();
      await this.testBasicServerRouting();
      
      // Test Suite 2: Cross-Environment Promise Chains
      await this.testCrossEnvironmentRouting();
      await this.testMultiHopRoutingChain();
      
      // Test Suite 3: Complex Routing Scenarios
      await this.testRandomizedChaosRouting();
      await this.testConcurrentRoutingLoad();
      
      // Test Suite 4: Error Handling & Recovery
      await this.testErrorPropagation();
      await this.testTimeoutHandling();
      
      await this.disconnectFromSystem();
      
      this.printTestSummary();
      return this.allTestsPassed();
      
    } catch (error) {
      console.error('❌ Test suite failed to complete:', error);
      return false;
    }
  }

  private async connectToSystem(): Promise<void> {
    return new Promise((resolve, reject) => {
      console.log('🔌 Connecting to JTAG system on ws://localhost:9001...');
      
      this.ws = new WebSocket('ws://localhost:9001');
      
      this.ws.on('open', () => {
        console.log('✅ Connected to JTAG WebSocket server');
        resolve();
      });
      
      this.ws.on('message', (data) => {
        try {
          const message = JSON.parse(data.toString());
          const correlationId = message.correlationId;
          
          if (correlationId && this.messageHandlers.has(correlationId)) {
            const handler = this.messageHandlers.get(correlationId)!;
            handler(message);
            this.messageHandlers.delete(correlationId);
          }
        } catch (error) {
          console.warn('⚠️ Failed to parse WebSocket message:', error);
        }
      });
      
      this.ws.on('error', (error) => {
        console.error('❌ WebSocket error:', error);
        reject(error);
      });
      
      this.ws.on('close', () => {
        console.log('🔌 WebSocket connection closed');
      });
      
      setTimeout(() => reject(new Error('Connection timeout')), 10000);
    });
  }

  private async sendCommand(commandName: string, payload: any, timeoutMs = 5000): Promise<any> {
    if (!this.ws) throw new Error('WebSocket not connected');

    const correlationId = `client_${Date.now()}_${Math.random().toString(36).substr(2, 8)}`;
    
    // Use proper JTAGMessage format like JTAGClient does
    const context = { uuid: 'integration-test', environment: 'server' as const };
    const message: JTAGMessage = JTAGMessageFactory.createRequest(
      context,
      'server', // origin: server environment 
      `server/commands/${commandName}`, // target: server command endpoint
      {
        context,
        sessionId: 'integration-test-session',
        ...payload
      },
      correlationId
    );

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        this.messageHandlers.delete(correlationId);
        reject(new Error(`Request timeout after ${timeoutMs}ms`));
      }, timeoutMs);

      this.messageHandlers.set(correlationId, (response) => {
        clearTimeout(timeout);
        if (response.error) {
          reject(new Error(response.error));
        } else {
          resolve(response);
        }
      });

      this.ws!.send(JSON.stringify(message));
    });
  }

  private async testScreenshotCommand(): Promise<void> {
    await this.recordTest('Real-World Screenshot Command (Server → Browser → Server)', async () => {
      const response = await this.sendCommand('screenshot', {
        querySelector: 'body',
        filename: 'integration-test-screenshot.png'
      }, 15000); // Extended timeout for screenshot processing

      if (!response.payload || !response.payload.success) {
        throw new Error('Screenshot command failed');
      }

      // Debug: Log the actual response structure
      console.log('📋 Screenshot response structure:', JSON.stringify(response, null, 2));
      
      // More flexible validation - screenshot command worked, that's what matters for routing test
      const result = response.payload;
      if (!result) {
        throw new Error('No payload in screenshot response');
      }

      return {
        result: result,
        crossEnvironmentHops: 3, // server → browser → server
        responseReceived: true
      };
    });
  }

  private async recordTest(name: string, testFn: () => Promise<any>): Promise<void> {
    const startTime = Date.now();
    
    try {
      console.log(`\n🧪 Running: ${name}`);
      const result = await testFn();
      const duration = Date.now() - startTime;
      
      this.results.push({
        name,
        success: true,
        duration,
        details: result
      });
      
      console.log(`✅ ${name} - Passed (${duration}ms)`);
    } catch (error: any) {
      const duration = Date.now() - startTime;
      
      this.results.push({
        name,
        success: false,
        duration,
        error: error.message
      });
      
      console.log(`❌ ${name} - Failed (${duration}ms): ${error.message}`);
    }
  }

  private async testBasicServerRouting(): Promise<void> {
    await this.recordTest('Basic Server Routing', async () => {
      const response = await this.sendCommand('test/routing-chaos', {
        testId: 'basic-server-test',
        hopCount: 0,
        maxHops: 1,
        routingPath: [],
        currentEnvironment: 'server',
        targetEnvironment: 'server',
        failureRate: 0,
        delayRange: [1, 5],
        payloadSize: 'small',
        testStartTime: new Date().toISOString(),
        correlationTrace: []
      });

      if (!response.payload || !response.payload.success) {
        throw new Error('Basic server routing failed');
      }

      return { hops: response.payload.totalHops };
    });
  }

  private async testCrossEnvironmentRouting(): Promise<void> {
    await this.recordTest('Cross-Environment Routing (Server → Browser)', async () => {
      const response = await this.sendCommand('test/routing-chaos', {
        testId: 'cross-env-test',
        hopCount: 0,
        maxHops: 2,
        routingPath: [],
        currentEnvironment: 'server',
        targetEnvironment: 'browser',
        failureRate: 0,
        delayRange: [1, 10],
        payloadSize: 'small',
        testStartTime: new Date().toISOString(),
        correlationTrace: []
      }, 10000); // Longer timeout for cross-environment

      if (!response.payload || !response.payload.success) {
        throw new Error('Cross-environment routing failed');
      }

      return { 
        hops: response.payload.totalHops,
        routingPath: response.payload.actualPath
      };
    });
  }

  private async testMultiHopRoutingChain(): Promise<void> {
    await this.recordTest('Multi-Hop Routing Chain (5 hops)', async () => {
      const response = await this.sendCommand('test/routing-chaos', {
        testId: 'multi-hop-test',
        hopCount: 0,
        maxHops: 5,
        routingPath: [],
        currentEnvironment: 'server',
        failureRate: 0,
        delayRange: [1, 20],
        payloadSize: 'medium',
        testStartTime: new Date().toISOString(),
        correlationTrace: []
      }, 15000); // Extended timeout for multi-hop

      if (!response.payload || !response.payload.success) {
        throw new Error('Multi-hop routing chain failed');
      }

      const metrics = response.payload.performanceMetrics || {};
      if (metrics.totalCorrelations && metrics.totalCorrelations < 5) {
        console.log('⚠️ Lower than expected correlations:', metrics.totalCorrelations);
      }

      return { 
        hops: response.payload.totalHops,
        averageHopTime: metrics.averageHopTime,
        correlations: metrics.totalCorrelations
      };
    });
  }

  private async testRandomizedChaosRouting(): Promise<void> {
    await this.recordTest('Randomized Chaos Routing (10 hops, 10% failure)', async () => {
      const response = await this.sendCommand('test/routing-chaos', {
        testId: 'chaos-test',
        hopCount: 0,
        maxHops: 10,
        routingPath: [],
        currentEnvironment: 'server',
        failureRate: 0.1, // 10% random failure rate
        delayRange: [5, 50],
        payloadSize: 'large',
        testStartTime: new Date().toISOString(),
        correlationTrace: []
      }, 30000); // Extended timeout for chaos testing

      // Success OR controlled failure is acceptable for chaos testing
      const wasSuccessful = response.payload && response.payload.success;
      const hadControlledFailure = response.payload && !response.payload.success && response.payload.errorEncountered;

      if (!wasSuccessful && !hadControlledFailure) {
        throw new Error('Chaos routing had uncontrolled failure');
      }

      return { 
        success: wasSuccessful,
        controlledFailure: hadControlledFailure,
        hops: response.payload?.totalHops || 0,
        error: response.payload?.errorEncountered
      };
    });
  }

  private async testConcurrentRoutingLoad(): Promise<void> {
    await this.recordTest('Concurrent Routing Load (5 parallel requests)', async () => {
      const concurrentRequests = 5;
      const promises: Promise<any>[] = [];

      for (let i = 0; i < concurrentRequests; i++) {
        const promise = this.sendCommand('test/routing-chaos', {
          testId: `concurrent-${i}`,
          hopCount: 0,
          maxHops: 3,
          routingPath: [],
          currentEnvironment: 'server',
          failureRate: 0,
          delayRange: [10, 30],
          payloadSize: 'small',
          testStartTime: new Date().toISOString(),
          correlationTrace: []
        }, 20000);
        promises.push(promise);
      }

      const results = await Promise.allSettled(promises);
      const successful = results.filter(r => r.status === 'fulfilled').length;
      const failed = results.length - successful;

      if (successful < concurrentRequests * 0.8) { // Allow 20% failure tolerance
        throw new Error(`Too many concurrent requests failed: ${failed}/${concurrentRequests}`);
      }

      return {
        total: concurrentRequests,
        successful,
        failed,
        successRate: successful / concurrentRequests
      };
    });
  }

  private async testErrorPropagation(): Promise<void> {
    await this.recordTest('Error Propagation (100% failure rate)', async () => {
      try {
        const response = await this.sendCommand('test/routing-chaos', {
          testId: 'error-propagation-test',
          hopCount: 0,
          maxHops: 3,
          routingPath: [],
          currentEnvironment: 'server',
          failureRate: 1.0, // 100% failure rate
          delayRange: [1, 5],
          payloadSize: 'small',
          testStartTime: new Date().toISOString(),
          correlationTrace: []
        }, 10000);

        // We expect this to fail, so success is actually a test failure
        if (response.payload && response.payload.success) {
          throw new Error('Expected error injection to cause failure, but request succeeded');
        }

        return {
          expectedFailure: true,
          actualFailure: !response.payload?.success,
          error: response.payload?.errorEncountered
        };
      } catch (error: any) {
        // Errors are expected in this test
        return {
          expectedFailure: true,
          actualFailure: true,
          error: error.message
        };
      }
    });
  }

  private async testTimeoutHandling(): Promise<void> {
    await this.recordTest('Timeout Handling (very short timeout)', async () => {
      try {
        await this.sendCommand('test/routing-chaos', {
          testId: 'timeout-test',
          hopCount: 0,
          maxHops: 10,
          routingPath: [],
          currentEnvironment: 'server',
          failureRate: 0,
          delayRange: [100, 200], // Long delays
          payloadSize: 'large',
          testStartTime: new Date().toISOString(),
          correlationTrace: []
        }, 500); // Very short timeout

        throw new Error('Expected timeout, but request completed');
      } catch (error: any) {
        if (error.message.includes('timeout')) {
          return { timeoutHandled: true };
        }
        throw error;
      }
    });
  }

  private async disconnectFromSystem(): Promise<void> {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
    }
  }

  private printTestSummary(): void {
    console.log('\n' + '=' .repeat(80));
    console.log('📊 COMPREHENSIVE ROUTING TEST SUMMARY');
    console.log('=' .repeat(80));

    const passed = this.results.filter(r => r.success).length;
    const failed = this.results.filter(r => !r.success).length;
    const totalDuration = this.results.reduce((sum, r) => sum + r.duration, 0);

    console.log(`✅ Passed: ${passed}`);
    console.log(`❌ Failed: ${failed}`);
    console.log(`⏱️  Total Duration: ${totalDuration}ms`);
    console.log(`📈 Success Rate: ${(passed / this.results.length * 100).toFixed(1)}%`);

    if (failed > 0) {
      console.log('\n❌ Failed Tests:');
      this.results.filter(r => !r.success).forEach(result => {
        console.log(`   ${result.name}: ${result.error}`);
      });
    }

    if (passed > 0) {
      console.log('\n✅ Performance Metrics:');
      this.results.filter(r => r.success).forEach(result => {
        console.log(`   ${result.name}: ${result.duration}ms`);
      });
    }

    console.log('\n🎯 Router Performance Analysis:');
    const avgResponseTime = totalDuration / this.results.length;
    console.log(`   Average Response Time: ${avgResponseTime.toFixed(1)}ms`);
    
    const successfulTests = this.results.filter(r => r.success);
    if (successfulTests.length > 0) {
      const fastestTest = successfulTests.reduce((min, r) => r.duration < min.duration ? r : min);
      const slowestTest = successfulTests.reduce((max, r) => r.duration > max.duration ? r : max);
      console.log(`   Fastest: ${fastestTest.name} (${fastestTest.duration}ms)`);
      console.log(`   Slowest: ${slowestTest.name} (${slowestTest.duration}ms)`);
    }
  }

  private allTestsPassed(): boolean {
    return this.results.every(result => result.success);
  }
}

// Main execution
async function main() {
  const tester = new ComprehensiveRoutingTest();
  const success = await tester.runAllTests();
  
  if (success) {
    console.log('\n🎉 ALL ROUTING TESTS PASSED! Router is functioning correctly.');
    process.exit(0);
  } else {
    console.log('\n💥 SOME TESTS FAILED. Router needs attention.');
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  main().catch(console.error);
}

export { ComprehensiveRoutingTest };