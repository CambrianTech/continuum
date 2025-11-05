/**
 * Layer 2 Unit Test: WebSocketDaemon Message Type
 * 
 * Middle-Out Test: Unit test for the daemon process message handling
 * Validates WebSocketDaemon generates correct message types without full system
 * 
 * Process Layer: Tests daemon in isolation with mocked dependencies
 */

import { WebSocketDaemon } from '../../WebSocketDaemon';

interface MockCommandProcessor {
  name: string;
  handleMessage: (message: any) => Promise<any>;
}

interface TestCase {
  name: string;
  commandSuccess: boolean;
  expectedMessageType: string;
  expectedSuccess: boolean;
}

class WebSocketDaemonUnitTest {
  private daemon: WebSocketDaemon | null = null;
  private mockProcessor: MockCommandProcessor;
  private testResults: { name: string; passed: boolean; error?: string }[] = [];

  constructor() {
    this.mockProcessor = {
      name: 'command-processor',
      handleMessage: async (message: any) => {
        // Mock successful console command response
        if (message.data.command === 'console') {
          return {
            success: true,
            data: {
              forwarded: true,
              timestamp: new Date().toISOString(),
              consoleEntry: { action: 'test', message: 'test' }
            }
          };
        }
        
        // Mock failure for unknown commands
        return {
          success: false,
          error: 'Command not found'
        };
      }
    };
  }

  async runAllTests(): Promise<void> {
    console.log('🧪 Layer 2 Unit: WebSocketDaemon Message Type Generation');
    console.log('=' .repeat(60));

    await this.setupDaemon();

    const testCases: TestCase[] = [
      {
        name: 'Success response uses execute_command_response',
        commandSuccess: true,
        expectedMessageType: 'execute_command_response',
        expectedSuccess: true
      },
      {
        name: 'Error response uses execute_command_response',
        commandSuccess: false,
        expectedMessageType: 'execute_command_response', 
        expectedSuccess: false
      }
    ];

    for (const testCase of testCases) {
      await this.runTestCase(testCase);
    }

    await this.teardownDaemon();
    this.reportResults();
  }

  private async setupDaemon(): Promise<void> {
    try {
      this.daemon = new WebSocketDaemon({ port: 9002, host: 'localhost' });
      this.daemon.registerDaemon(this.mockProcessor);
      console.log('✅ Daemon setup complete');
    } catch (error) {
      console.error('❌ Daemon setup failed:', error);
      throw error;
    }
  }

  private async teardownDaemon(): Promise<void> {
    if (this.daemon) {
      try {
        await this.daemon.stop();
        console.log('✅ Daemon teardown complete');
      } catch (error) {
        console.error('⚠️ Daemon teardown error:', error);
      }
    }
  }

  private async runTestCase(testCase: TestCase): Promise<void> {
    try {
      // Mock WebSocket connection and message data
      const mockConnectionId = 'test-connection-123';
      const mockMessage = {
        type: 'execute_command',
        data: {
          command: testCase.commandSuccess ? 'console' : 'unknown-command',
          params: JSON.stringify({ action: 'test', message: 'unit test' }),
          requestId: 'test-request-123',
          sessionId: 'test-session'
        }
      };

      // Mock WebSocket manager to capture sent messages
      let capturedResponse: any = null;
      const mockWsManager = {
        sendToConnection: (connectionId: string, response: any) => {
          capturedResponse = response;
        }
      };

      // Replace the WebSocket manager temporarily (this is a unit test hack)
      (this.daemon as any).wsManager = mockWsManager;

      // Execute the command routing method directly
      await (this.daemon as any).routeCommandToProcessor(mockConnectionId, mockMessage);

      // Validate the captured response
      if (!capturedResponse) {
        this.testResults.push({
          name: testCase.name,
          passed: false,
          error: 'No response captured'
        });
        return;
      }

      const typeMatch = capturedResponse.type === testCase.expectedMessageType;
      const successMatch = capturedResponse.success === testCase.expectedSuccess;

      if (typeMatch && successMatch) {
        this.testResults.push({
          name: testCase.name,
          passed: true
        });
      } else {
        this.testResults.push({
          name: testCase.name,
          passed: false,
          error: `Expected type=${testCase.expectedMessageType} success=${testCase.expectedSuccess}, got type=${capturedResponse.type} success=${capturedResponse.success}`
        });
      }

    } catch (error) {
      this.testResults.push({
        name: testCase.name,
        passed: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  private reportResults(): void {
    console.log('\\n📊 Unit Test Results:');
    console.log('-'.repeat(60));

    let passed = 0;
    let failed = 0;

    for (const result of this.testResults) {
      const status = result.passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${status} ${result.name}`);
      
      if (!result.passed && result.error) {
        console.log(`     Error: ${result.error}`);
      }

      result.passed ? passed++ : failed++;
    }

    console.log('-'.repeat(60));
    console.log(`📈 Summary: ${passed} passed, ${failed} failed`);
    
    if (failed === 0) {
      console.log('🎉 All Layer 2 unit tests PASSED! WebSocketDaemon message type generation validated.');
    } else {
      console.log('❌ Some unit tests FAILED. Check WebSocketDaemon.routeCommandToProcessor implementation.');
    }
  }
}

// Execute tests if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const unitTest = new WebSocketDaemonUnitTest();
  unitTest.runAllTests().catch(error => {
    console.error('❌ Unit test execution failed:', error);
    process.exit(1);
  });
}