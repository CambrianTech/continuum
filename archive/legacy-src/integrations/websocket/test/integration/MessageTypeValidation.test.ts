/**
 * Layer 4 Integration Test: WebSocket Message Type Validation
 * 
 * Middle-Out Test: Validates the fix for browser console logging issue
 * Tests that WebSocketDaemon sends 'execute_command_response' not 'command_response'
 * 
 * Integration Layer: WebSocket + Command Processor + Console Command
 */

import WebSocket from 'ws';

interface TestResult {
  name: string;
  passed: boolean;
  error?: string;
  details?: any;
}

class MessageTypeValidator {
  private results: TestResult[] = [];

  async runAllTests(): Promise<void> {
    console.log('🧪 Layer 4 Integration: WebSocket Message Type Validation');
    console.log('=' .repeat(60));

    await this.testCorrectMessageType();
    await this.testErrorMessageType();
    await this.testConsoleCommandFlow();

    this.reportResults();
  }

  private async testCorrectMessageType(): Promise<void> {
    const testName = 'WebSocket sends execute_command_response (not command_response)';
    
    try {
      const result = await this.executeTest({
        command: 'console',
        params: {
          action: 'log',
          message: 'Message type validation test',
          source: 'integration-test'
        }
      });

      if (result.type === 'execute_command_response') {
        this.results.push({ 
          name: testName, 
          passed: true,
          details: { messageType: result.type, command: result.command }
        });
      } else {
        this.results.push({ 
          name: testName, 
          passed: false, 
          error: `Expected 'execute_command_response', got '${result.type}'`,
          details: result
        });
      }
    } catch (error) {
      this.results.push({ 
        name: testName, 
        passed: false, 
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  private async testErrorMessageType(): Promise<void> {
    const testName = 'Error responses also use execute_command_response';
    
    try {
      const result = await this.executeTest({
        command: 'nonexistent-command',
        params: {}
      });

      if (result.type === 'execute_command_response' && result.success === false) {
        this.results.push({ 
          name: testName, 
          passed: true,
          details: { messageType: result.type, success: result.success }
        });
      } else {
        this.results.push({ 
          name: testName, 
          passed: false, 
          error: `Expected 'execute_command_response' with success=false, got type='${result.type}' success=${result.success}`,
          details: result
        });
      }
    } catch (error) {
      this.results.push({ 
        name: testName, 
        passed: false, 
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  private async testConsoleCommandFlow(): Promise<void> {
    const testName = 'Console command complete integration flow';
    
    try {
      const result = await this.executeTest({
        command: 'console',
        params: {
          action: 'health_report',
          message: 'Layer 4 integration validation',
          source: 'middle-out-test'
        }
      });

      const valid = (
        result.type === 'execute_command_response' &&
        result.success === true &&
        result.command === 'console' &&
        result.data?.forwarded === true &&
        result.data?.consoleEntry?.action === 'health_report'
      );

      if (valid) {
        this.results.push({ 
          name: testName, 
          passed: true,
          details: { 
            messageType: result.type,
            forwarded: result.data.forwarded,
            action: result.data.consoleEntry.action
          }
        });
      } else {
        this.results.push({ 
          name: testName, 
          passed: false, 
          error: 'Console command integration flow validation failed',
          details: result
        });
      }
    } catch (error) {
      this.results.push({ 
        name: testName, 
        passed: false, 
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }

  private async executeTest(testCommand: { command: string; params: any }): Promise<any> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket('ws://localhost:9000');
      const requestId = `test-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
      let responseReceived = false;

      const timeout = setTimeout(() => {
        if (!responseReceived) {
          ws.close();
          reject(new Error('Test timeout'));
        }
      }, 5000);

      ws.on('open', () => {
        const message = {
          type: 'execute_command',
          data: {
            command: testCommand.command,
            params: JSON.stringify(testCommand.params),
            requestId,
            sessionId: 'integration-test-session'
          }
        };

        ws.send(JSON.stringify(message));
      });

      ws.on('message', (data) => {
        try {
          const response = JSON.parse(data.toString());
          
          // Skip connection confirmation
          if (response.type === 'connection_confirmed') {
            return;
          }

          if (response.requestId === requestId) {
            responseReceived = true;
            clearTimeout(timeout);
            ws.close();
            resolve(response);
          }
        } catch (error) {
          clearTimeout(timeout);
          ws.close();
          reject(error);
        }
      });

      ws.on('error', (error) => {
        clearTimeout(timeout);
        reject(error);
      });
    });
  }

  private reportResults(): void {
    console.log('\\n📊 Test Results:');
    console.log('-'.repeat(60));

    let passed = 0;
    let failed = 0;

    for (const result of this.results) {
      const status = result.passed ? '✅ PASS' : '❌ FAIL';
      console.log(`${status} ${result.name}`);
      
      if (!result.passed && result.error) {
        console.log(`     Error: ${result.error}`);
      }
      
      if (result.details) {
        console.log(`     Details:`, JSON.stringify(result.details, null, 8));
      }

      result.passed ? passed++ : failed++;
    }

    console.log('-'.repeat(60));
    console.log(`📈 Summary: ${passed} passed, ${failed} failed`);
    
    if (failed === 0) {
      console.log('🎉 All Layer 4 integration tests PASSED! Browser console logging fix validated.');
    } else {
      console.log('❌ Some tests FAILED. Check WebSocket message type implementation.');
    }
  }
}

// Execute tests if run directly
if (import.meta.url === `file://${process.argv[1]}`) {
  const validator = new MessageTypeValidator();
  validator.runAllTests().catch(error => {
    console.error('❌ Test execution failed:', error);
    process.exit(1);
  });
}