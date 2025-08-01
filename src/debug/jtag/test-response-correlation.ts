#!/usr/bin/env tsx
/**
 * Response Correlation Detection Test
 * 
 * Tests that all JTAG commands properly handle request-response correlation
 * without generating orphaned responses or correlation warnings.
 */

import WebSocket from 'ws';
import { JTAGMessageFactory } from './shared/JTAGTypes';

interface TestResult {
  command: string;
  correlationId: string;
  success: boolean;
  receivedResponse: boolean;
  orphanedResponse: boolean;
  duplicateResponse: boolean;
  errorMessage?: string;
}

class ResponseCorrelationTester {
  private ws?: WebSocket;
  private testResults: TestResult[] = [];
  private pendingRequests = new Map<string, {
    command: string;
    startTime: number;
    resolved: boolean;
    responseCount: number;
  }>();

  // Commands to test with minimal safe parameters
  private readonly TEST_COMMANDS = [
    { name: 'ping', params: { message: 'correlation-test' } },
    { name: 'list', params: { category: 'all' } },
    // Add more commands with minimal safe parameters
    { name: 'file/save', params: { 
      filename: 'correlation-test.txt', 
      content: 'test content',
      ensureDirectory: true 
    }},
    // We'll skip potentially destructive commands like navigate, click, etc.
  ];

  async runCorrelationTest(): Promise<TestResult[]> {
    console.log('🧪 Response Correlation Detection Test');
    console.log(`📋 Testing ${this.TEST_COMMANDS.length} commands for correlation issues...`);

    try {
      await this.connectWebSocket();
      await this.executeCommandTests();
      await this.waitForPendingResponses();
      this.analyzeResults();
      
    } catch (error) {
      console.error('❌ Correlation test failed:', error);
    } finally {
      this.closeWebSocket();
    }

    return this.testResults;
  }

  private async connectWebSocket(): Promise<void> {
    return new Promise((resolve, reject) => {
      console.log('🔗 Connecting to WebSocket server...');
      this.ws = new WebSocket('ws://localhost:9001');

      this.ws.on('open', () => {
        console.log('✅ WebSocket connected');
        resolve();
      });

      this.ws.on('message', (data) => {
        this.handleResponse(data.toString());
      });

      this.ws.on('error', (error) => {
        console.error('❌ WebSocket error:', error.message);
        reject(error);
      });

      // Timeout after 5 seconds
      setTimeout(() => {
        reject(new Error('WebSocket connection timeout'));
      }, 5000);
    });
  }

  private async executeCommandTests(): Promise<void> {
    console.log('⚡ Executing command correlation tests...');

    for (const testCommand of this.TEST_COMMANDS) {
      await this.testCommandCorrelation(testCommand.name, testCommand.params);
      // Small delay between commands to avoid overwhelming the system
      await new Promise(resolve => setTimeout(resolve, 100));
    }
  }

  private async testCommandCorrelation(commandName: string, params: any): Promise<void> {
    const correlationId = `correlation_test_${Date.now()}_${Math.random().toString(36).substr(2, 8)}`;
    
    console.log(`🎯 Testing ${commandName} correlation...`);

    // Track this request
    this.pendingRequests.set(correlationId, {
      command: commandName,
      startTime: Date.now(),
      resolved: false,
      responseCount: 0
    });

    // Create JTAG request message
    const context = { uuid: 'correlation-test-session', environment: 'server' as const };
    const payload = {
      sessionId: 'correlation-test-session',
      context: context,
      ...params
    };

    const message = JTAGMessageFactory.createRequest(
      context,
      'client',
      `commands/${commandName}`,
      payload,
      correlationId
    );

    // Send the message
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    } else {
      throw new Error('WebSocket not connected');
    }
  }

  private handleResponse(data: string): void {
    try {
      const response = JSON.parse(data);
      const correlationId = response.correlationId;

      if (!correlationId) {
        console.log('📨 Received message without correlation ID');
        return;
      }

      const pendingRequest = this.pendingRequests.get(correlationId);
      
      if (pendingRequest) {
        // This is expected - we have a pending request for this correlation ID
        pendingRequest.responseCount++;
        
        if (pendingRequest.responseCount === 1) {
          pendingRequest.resolved = true;
          console.log(`✅ ${pendingRequest.command}: Response received (${Date.now() - pendingRequest.startTime}ms)`);
        } else {
          // Duplicate response detected!
          console.log(`⚠️ ${pendingRequest.command}: Duplicate response #${pendingRequest.responseCount}`);
        }
      } else {
        // Orphaned response - this indicates a correlation issue
        console.log(`🚨 Orphaned response for correlation ID: ${correlationId}`);
        
        // Try to determine which command this was for based on the response structure
        const commandName = this.guessCommandFromResponse(response);
        
        this.testResults.push({
          command: commandName || 'unknown',
          correlationId,
          success: false,
          receivedResponse: true,
          orphanedResponse: true,
          duplicateResponse: false,
          errorMessage: 'Response received without matching pending request (orphaned response)'
        });
      }
    } catch (error) {
      console.error('❌ Failed to parse response:', error);
    }
  }

  private guessCommandFromResponse(response: any): string | null {
    // Try to determine command from response structure
    if (response.origin && response.origin.includes('commands/')) {
      return response.origin.replace('commands/', '');
    }
    
    if (response.payload && response.payload.commandResult) {
      // Look for command-specific indicators in the result
      const result = response.payload.commandResult;
      if (result.message === 'pong') return 'ping';
      if (result.commands && Array.isArray(result.commands)) return 'list';
      if (result.filePath) return 'file/save';
    }
    
    return null;
  }

  private async waitForPendingResponses(): Promise<void> {
    console.log('⏳ Waiting for pending responses...');
    
    // Wait up to 10 seconds for all responses
    const maxWaitTime = 10000;
    const checkInterval = 100;
    let waitedTime = 0;

    while (waitedTime < maxWaitTime) {
      const stillPending = Array.from(this.pendingRequests.values())
        .filter(req => !req.resolved);
      
      if (stillPending.length === 0) {
        console.log('✅ All responses received');
        break;
      }

      await new Promise(resolve => setTimeout(resolve, checkInterval));
      waitedTime += checkInterval;
    }

    // Check for timeouts
    const timedOut = Array.from(this.pendingRequests.entries())
      .filter(([_, req]) => !req.resolved);
    
    if (timedOut.length > 0) {
      console.log(`⚠️ ${timedOut.length} commands timed out without responses`);
    }
  }

  private analyzeResults(): void {
    console.log('\n📊 Response Correlation Analysis');
    
    // Create results for all tested commands
    for (const [correlationId, request] of this.pendingRequests) {
      const result: TestResult = {
        command: request.command,
        correlationId,
        success: request.resolved && request.responseCount === 1,
        receivedResponse: request.responseCount > 0,
        orphanedResponse: false, // These are tracked requests, so not orphaned
        duplicateResponse: request.responseCount > 1,
        errorMessage: request.resolved ? 
          (request.responseCount > 1 ? `Received ${request.responseCount} duplicate responses` : undefined) :
          'Command timed out without response'
      };

      this.testResults.push(result);
    }

    // Print summary
    const totalCommands = this.testResults.length;
    const successfulCommands = this.testResults.filter(r => r.success).length;
    const orphanedResponses = this.testResults.filter(r => r.orphanedResponse).length;
    const duplicateResponses = this.testResults.filter(r => r.duplicateResponse).length;
    const timeouts = this.testResults.filter(r => !r.receivedResponse).length;

    console.log(`\n📋 Summary:`);
    console.log(`   Total Commands Tested: ${totalCommands}`);
    console.log(`   ✅ Successful: ${successfulCommands}`);
    console.log(`   🚨 Orphaned Responses: ${orphanedResponses}`);
    console.log(`   📋 Duplicate Responses: ${duplicateResponses}`);
    console.log(`   ⏰ Timeouts: ${timeouts}`);

    // Print detailed results for problematic commands
    const problematicResults = this.testResults.filter(r => !r.success);
    if (problematicResults.length > 0) {
      console.log(`\n🚨 Problematic Commands:`);
      for (const result of problematicResults) {
        console.log(`   ${result.command}: ${result.errorMessage}`);
      }
    }

    if (successfulCommands === totalCommands) {
      console.log(`\n🎉 All commands passed correlation testing!`);
    } else {
      console.log(`\n⚠️  ${totalCommands - successfulCommands} commands have correlation issues`);
    }
  }

  private closeWebSocket(): void {
    if (this.ws) {
      this.ws.close();
      console.log('🔌 WebSocket closed');
    }
  }
}

// Run the test
async function main() {
  const tester = new ResponseCorrelationTester();
  const results = await tester.runCorrelationTest();
  
  // Exit with appropriate code
  const hasIssues = results.some(r => !r.success);
  process.exit(hasIssues ? 1 : 0);
}

if (require.main === module) {
  main().catch(error => {
    console.error('❌ Test failed:', error);
    process.exit(1);
  });
}