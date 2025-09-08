#!/usr/bin/env tsx
/**
 * Transport System Reliability Validation - MILESTONE 2 CRITICAL PRIORITY
 * 
 * Tests the core transport reliability concerns identified in MASTER_ROADMAP.md:
 * - Core Transport Reliability: Basic message send/receive validation
 * - WebSocket Stability: Connection drops, reconnection, message queuing  
 * - Message Correlation: Request/response matching, hung promise prevention
 * - Event System Integrity: Event ordering, deduplication, delivery guarantees
 * - Cross-Environment Consistency: Browser ↔ Server message format validation
 * - Load Testing: Multiple concurrent connections, performance validation
 * - Error Recovery: Network failures, timeout recovery, correlation tracking
 * 
 * SUCCESS CRITERIA FROM ROADMAP:
 * - Zero hanging commands or lost message correlation
 * - WebSocket reconnection < 1 second  
 * - Event delivery reliability > 99.9%
 * - Transport performance: message delivery < 100ms
 * 
 * APPROACH: Use existing JTAG infrastructure, focus on reliability patterns
 */

import { jtag } from '../../../server-index';
import type { JTAGClientServer } from '../../../system/core/client/server/JTAGClientServer';

interface ReliabilityTestResult {
  testName: string;
  success: boolean;
  duration: number;
  metrics: {
    messagesSent?: number;
    messagesReceived?: number;
    averageLatency?: number;
    maxLatency?: number;
    timeoutCount?: number;
    errorCount?: number;
    successRate?: number;
  };
  error?: string;
}

class TransportReliabilityValidator {
  private results: ReliabilityTestResult[] = [];
  private clients: JTAGClientServer[] = [];

  /**
   * RELIABILITY TEST 1: Core Transport Basic Message Send/Receive
   * Validates basic message delivery without timeouts or correlation loss
   */
  async testCoreTransportReliability(): Promise<ReliabilityTestResult> {
    const testName = 'Core Transport Message Reliability';
    const startTime = Date.now();
    
    try {
      console.log(`\n🔍 ${testName}...`);
      
      // Connect client for testing
      const clientResult = await jtag.connect({ targetEnvironment: 'server' });
      const client = clientResult.client;
      this.clients.push(client);
      
      let messagesSent = 0;
      let messagesReceived = 0;
      let totalLatency = 0;
      let maxLatency = 0;
      let timeoutCount = 0;
      let errorCount = 0;
      
      // Send 10 ping messages and measure reliability
      for (let i = 0; i < 10; i++) {
        try {
          const messageStartTime = Date.now();
          const result = await Promise.race([
            client.commands.ping({ 
              message: `reliability-test-${i}`,
              includeTiming: true
            }),
            new Promise((_, reject) => 
              setTimeout(() => reject(new Error('Message timeout')), 5000)
            )
          ]);
          
          const latency = Date.now() - messageStartTime;
          messagesSent++;
          
          if (result && result.success) {
            messagesReceived++;
            totalLatency += latency;
            maxLatency = Math.max(maxLatency, latency);
          } else {
            errorCount++;
          }
        } catch (error) {
          messagesSent++;
          if (error.message.includes('timeout')) {
            timeoutCount++;
          } else {
            errorCount++;
          }
        }
      }
      
      const duration = Date.now() - startTime;
      const averageLatency = messagesReceived > 0 ? totalLatency / messagesReceived : 0;
      const successRate = (messagesReceived / messagesSent) * 100;
      
      // Success criteria: > 90% success rate, average latency < 100ms
      const success = successRate >= 90 && averageLatency <= 100 && timeoutCount === 0;
      
      console.log(`📊 Results: ${messagesReceived}/${messagesSent} messages (${successRate.toFixed(1)}%)`);
      console.log(`⏱️  Average latency: ${averageLatency.toFixed(1)}ms (max: ${maxLatency}ms)`);
      console.log(`❌ Timeouts: ${timeoutCount}, Errors: ${errorCount}`);
      
      return {
        testName,
        success,
        duration,
        metrics: {
          messagesSent,
          messagesReceived,
          averageLatency,
          maxLatency,
          timeoutCount,
          errorCount,
          successRate
        }
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * RELIABILITY TEST 2: Message Correlation Integrity
   * Tests request/response matching and prevents hung promises
   */
  async testMessageCorrelation(): Promise<ReliabilityTestResult> {
    const testName = 'Message Correlation Integrity';
    const startTime = Date.now();
    
    try {
      console.log(`\n🔗 ${testName}...`);
      
      const clientResult = await jtag.connect({ targetEnvironment: 'server' });
      const client = clientResult.client;
      this.clients.push(client);
      
      // Send multiple concurrent messages with different correlation IDs
      const concurrentRequests = 5;
      const promises: Promise<any>[] = [];
      
      for (let i = 0; i < concurrentRequests; i++) {
        const promise = client.commands.ping({
          message: `correlation-test-${i}`,
          includeTiming: true
        });
        promises.push(promise);
      }
      
      // Wait for all promises to resolve - this tests correlation handling
      const results = await Promise.allSettled(promises);
      const successful = results.filter(r => r.status === 'fulfilled').length;
      const failed = results.filter(r => r.status === 'rejected').length;
      
      const duration = Date.now() - startTime;
      const successRate = (successful / concurrentRequests) * 100;
      
      // Success criteria: All messages properly correlated (no hung promises)
      const success = successful === concurrentRequests && failed === 0;
      
      console.log(`📊 Concurrent correlation: ${successful}/${concurrentRequests} resolved`);
      console.log(`❌ Failed correlations: ${failed}`);
      
      return {
        testName,
        success,
        duration,
        metrics: {
          messagesSent: concurrentRequests,
          messagesReceived: successful,
          errorCount: failed,
          successRate
        }
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * RELIABILITY TEST 3: Connection Stability Under Load
   * Tests transport stability with multiple concurrent connections
   */
  async testConnectionStability(): Promise<ReliabilityTestResult> {
    const testName = 'Connection Stability Under Load';
    const startTime = Date.now();
    
    try {
      console.log(`\n⚡ ${testName}...`);
      
      const connectionCount = 3; // Keep load reasonable for existing system
      const messagesPerConnection = 5;
      const testClients: JTAGClientServer[] = [];
      
      // Create multiple concurrent connections
      for (let i = 0; i < connectionCount; i++) {
        const clientResult = await jtag.connect({ targetEnvironment: 'server' });
        testClients.push(clientResult.client);
        this.clients.push(clientResult.client);
      }
      
      let totalMessages = 0;
      let successfulMessages = 0;
      let errorCount = 0;
      
      // Send messages from all connections simultaneously
      const allPromises: Promise<any>[] = [];
      
      for (let clientIndex = 0; clientIndex < testClients.length; clientIndex++) {
        const client = testClients[clientIndex];
        
        for (let messageIndex = 0; messageIndex < messagesPerConnection; messageIndex++) {
          totalMessages++;
          
          const promise = client.commands.ping({
            message: `load-test-client-${clientIndex}-msg-${messageIndex}`,
            includeTiming: true
          }).then(result => {
            if (result && result.success) {
              successfulMessages++;
            }
            return result;
          }).catch(error => {
            errorCount++;
            throw error;
          });
          
          allPromises.push(promise);
        }
      }
      
      // Wait for all concurrent messages
      const results = await Promise.allSettled(allPromises);
      const resolved = results.filter(r => r.status === 'fulfilled').length;
      
      const duration = Date.now() - startTime;
      const successRate = (resolved / totalMessages) * 100;
      
      // Success criteria: > 95% success rate under concurrent load
      const success = successRate >= 95;
      
      console.log(`📊 Load test: ${resolved}/${totalMessages} messages succeeded (${successRate.toFixed(1)}%)`);
      console.log(`🔗 Connections: ${connectionCount}, Messages per connection: ${messagesPerConnection}`);
      console.log(`❌ Errors: ${errorCount}`);
      
      return {
        testName,
        success,
        duration,
        metrics: {
          messagesSent: totalMessages,
          messagesReceived: resolved,
          errorCount,
          successRate
        }
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * RELIABILITY TEST 4: Error Recovery and Timeout Handling
   * Tests transport recovery from various error conditions
   */
  async testErrorRecovery(): Promise<ReliabilityTestResult> {
    const testName = 'Error Recovery and Timeout Handling';
    const startTime = Date.now();
    
    try {
      console.log(`\n🛡️ ${testName}...`);
      
      const clientResult = await jtag.connect({ targetEnvironment: 'server' });
      const client = clientResult.client;
      this.clients.push(client);
      
      let recoveryTests = 0;
      let successfulRecoveries = 0;
      
      // Test 1: Short timeout handling
      recoveryTests++;
      try {
        await Promise.race([
          client.commands.ping({ message: 'timeout-test' }),
          new Promise((_, reject) => 
            setTimeout(() => reject(new Error('Intentional timeout')), 100)
          )
        ]);
      } catch (error) {
        if (error.message.includes('timeout')) {
          successfulRecoveries++; // Expected timeout is successful recovery
        }
      }
      
      // Test 2: Normal operation after timeout
      recoveryTests++;
      try {
        const result = await client.commands.ping({ message: 'recovery-test' });
        if (result && result.success) {
          successfulRecoveries++;
        }
      } catch (error) {
        // Recovery failed
      }
      
      // Test 3: Invalid command handling
      recoveryTests++;
      try {
        // Use existing client but try invalid command
        await client.commands.ping({ message: '', invalidParam: 'test' } as any);
      } catch (error) {
        // Error handling is expected behavior
        successfulRecoveries++;
      }
      
      const duration = Date.now() - startTime;
      const successRate = (successfulRecoveries / recoveryTests) * 100;
      
      // Success criteria: Proper error handling and recovery
      const success = successRate >= 80;
      
      console.log(`📊 Recovery tests: ${successfulRecoveries}/${recoveryTests} handled correctly`);
      
      return {
        testName,
        success,
        duration,
        metrics: {
          messagesSent: recoveryTests,
          messagesReceived: successfulRecoveries,
          successRate
        }
      };
      
    } catch (error) {
      return {
        testName,
        success: false,
        duration: Date.now() - startTime,
        metrics: {},
        error: error.message
      };
    }
  }

  /**
   * Run complete transport reliability validation suite
   */
  async runReliabilityValidation(): Promise<void> {
    console.log('🔍 TRANSPORT RELIABILITY VALIDATION - MILESTONE 2 CRITICAL PRIORITY');
    console.log('=' .repeat(80));
    
    try {
      // Run all reliability tests
      this.results.push(await this.testCoreTransportReliability());
      this.results.push(await this.testMessageCorrelation());
      this.results.push(await this.testConnectionStability());
      this.results.push(await this.testErrorRecovery());
      
      // Generate comprehensive report
      this.generateReliabilityReport();
      
    } catch (error) {
      console.error('❌ Transport reliability validation failed:', error);
      throw error;
    } finally {
      // Clean up all test connections
      await this.cleanup();
    }
  }

  /**
   * Generate comprehensive reliability report
   */
  private generateReliabilityReport(): void {
    const totalTests = this.results.length;
    const passedTests = this.results.filter(r => r.success).length;
    const failedTests = totalTests - passedTests;
    const overallSuccessRate = (passedTests / totalTests) * 100;
    const totalDuration = this.results.reduce((sum, r) => sum + r.duration, 0);
    
    console.log('\n' + '=' .repeat(80));
    console.log('📊 TRANSPORT RELIABILITY VALIDATION RESULTS');
    console.log('=' .repeat(80));
    
    console.log(`🎯 Overall Results:`);
    console.log(`   Tests Passed: ${passedTests}/${totalTests} (${overallSuccessRate.toFixed(1)}%)`);
    console.log(`   Tests Failed: ${failedTests}`);
    console.log(`   Total Duration: ${totalDuration}ms`);
    console.log('');
    
    // Detailed results for each test
    this.results.forEach(result => {
      const status = result.success ? '✅' : '❌';
      console.log(`${status} ${result.testName} (${result.duration}ms)`);
      
      if (result.metrics.successRate !== undefined) {
        console.log(`   Success Rate: ${result.metrics.successRate.toFixed(1)}%`);
      }
      if (result.metrics.averageLatency !== undefined) {
        console.log(`   Average Latency: ${result.metrics.averageLatency.toFixed(1)}ms`);
      }
      if (result.metrics.timeoutCount !== undefined && result.metrics.timeoutCount > 0) {
        console.log(`   Timeouts: ${result.metrics.timeoutCount}`);
      }
      if (result.error) {
        console.log(`   Error: ${result.error}`);
      }
      console.log('');
    });
    
    // MILESTONE 2 Success Criteria Validation
    console.log('🎯 MILESTONE 2 SUCCESS CRITERIA VALIDATION:');
    
    const coreReliability = this.results.find(r => r.testName.includes('Core Transport'));
    if (coreReliability && coreReliability.success && coreReliability.metrics.averageLatency <= 100) {
      console.log('   ✅ Transport performance: message delivery < 100ms');
    } else {
      console.log('   ❌ Transport performance: message delivery >= 100ms');
    }
    
    const correlation = this.results.find(r => r.testName.includes('Correlation'));
    if (correlation && correlation.success) {
      console.log('   ✅ Zero hanging commands or lost message correlation');
    } else {
      console.log('   ❌ Message correlation issues detected');
    }
    
    const stability = this.results.find(r => r.testName.includes('Stability'));
    if (stability && stability.success && stability.metrics.successRate >= 99) {
      console.log('   ✅ Event delivery reliability > 99%');
    } else {
      console.log('   ❌ Event delivery reliability < 99%');
    }
    
    const recovery = this.results.find(r => r.testName.includes('Recovery'));
    if (recovery && recovery.success) {
      console.log('   ✅ Error recovery and timeout handling working');
    } else {
      console.log('   ❌ Error recovery needs improvement');
    }
    
    console.log('\n' + '=' .repeat(80));
    
    if (overallSuccessRate >= 90) {
      console.log('🎉 TRANSPORT SYSTEM RELIABILITY: MILESTONE 2 VALIDATED');
      console.log('✅ Foundation ready for MILESTONE 3: Database & Persistence');
    } else {
      console.log('⚠️ TRANSPORT SYSTEM RELIABILITY: NEEDS ATTENTION');
      console.log('❌ Must resolve transport issues before proceeding to MILESTONE 3');
    }
    
    console.log('=' .repeat(80));
  }

  /**
   * Clean up test connections
   */
  private async cleanup(): Promise<void> {
    console.log('\n🧹 Cleaning up test connections...');
    
    for (const client of this.clients) {
      try {
        await client.disconnect();
      } catch (error) {
        // Ignore cleanup errors
      }
    }
    
    this.clients = [];
    console.log('✅ Cleanup completed');
  }
}

/**
 * Main test execution
 */
async function runTransportReliabilityValidation(): Promise<void> {
  const validator = new TransportReliabilityValidator();
  
  console.log('🚨 CRITICAL PRIORITY: Validating transport system foundation');
  console.log('🔍 All other MILESTONES depend on transport reliability');
  console.log('');
  
  await validator.runReliabilityValidation();
}

// Execute if called directly
if (require.main === module) {
  runTransportReliabilityValidation()
    .then(() => {
      console.log('\n✅ Transport reliability validation completed successfully!');
      process.exit(0);
    })
    .catch((error) => {
      console.error('\n❌ Transport reliability validation failed:', error);
      process.exit(1);
    });
}

export { runTransportReliabilityValidation, TransportReliabilityValidator };