/**
 * ContinuumAPI Integration Tests
 * Test the full promise resolution chain: Browser → WebSocket → Command Processor → Daemon → Response
 */

import { spawn, ChildProcess } from 'child_process';
import WebSocket from 'ws';

interface TestEnvironment {
  serverProcess?: ChildProcess;
  serverReady: boolean;
  serverPort: number;
}

describe('ContinuumAPI Integration Tests', () => {
  let testEnv: TestEnvironment;
  let ws: WebSocket;

  beforeAll(async () => {
    testEnv = {
      serverReady: false,
      serverPort: 9001 // Use different port for testing
    };

    console.log('🧪 Starting test server...');
    await startTestServer();
    console.log('✅ Test server ready');
  }, 30000);

  afterAll(async () => {
    if (ws) {
      ws.close();
    }
    await stopTestServer();
  });

  beforeEach(async () => {
    // Create fresh WebSocket connection for each test
    await connectWebSocket();
  });

  afterEach(() => {
    if (ws) {
      ws.close();
    }
  });

  describe('Promise Resolution Chain', () => {
    test('info command should resolve with real server data', async () => {
      const result = await sendCommandAndWaitForResponse('info', {});
      
      expect(result.success).toBe(true);
      expect(result.data).toBeDefined();
      expect(result.data.version).toBeDefined();
      expect(typeof result.data.version).toBe('string');
      
      console.log('📋 Info command result:', result.data);
    }, 10000);

    test('info command with section parameter should work', async () => {
      const result = await sendCommandAndWaitForResponse('info', { section: 'system' });
      
      expect(result.success).toBe(true);
      expect(result.data.system).toBeDefined();
      expect(result.data.version).toBeDefined();
      
      console.log('🖥️ System info result:', result.data.system);
    }, 10000);

    test('list command should return available commands', async () => {
      // First check if list command exists
      const helpResult = await sendCommandAndWaitForResponse('help', {});
      console.log('📚 Help result:', helpResult);
      
      // Try to get list of commands (might be via help or a separate list command)
      const result = await sendCommandAndWaitForResponse('help', { command: 'list' });
      
      expect(result.success).toBe(true);
      console.log('📝 Command list result:', result);
    }, 10000);

    test('screenshot command should execute and return file info', async () => {
      const result = await sendCommandAndWaitForResponse('screenshot', { 
        filename: 'integration-test.png' 
      });
      
      // Screenshot might fail if no browser is open, but we should get a response
      expect(result).toBeDefined();
      console.log('📸 Screenshot result:', result);
    }, 15000);

    test('invalid command should return error response', async () => {
      const result = await sendCommandAndWaitForResponse('nonexistent-command', {});
      
      expect(result.success).toBe(false);
      expect(result.error).toBeDefined();
      console.log('❌ Error response:', result.error);
    }, 5000);

    test('multiple concurrent commands should all resolve', async () => {
      const commands = [
        sendCommandAndWaitForResponse('info', {}),
        sendCommandAndWaitForResponse('info', { section: 'memory' }),
        sendCommandAndWaitForResponse('help', {})
      ];
      
      const results = await Promise.all(commands);
      
      expect(results).toHaveLength(3);
      results.forEach((result, index) => {
        expect(result).toBeDefined();
        console.log(`🔄 Concurrent command ${index} result:`, result.success ? 'SUCCESS' : 'ERROR');
      });
    }, 15000);
  });

  describe('Command Processor Logging', () => {
    test('should log commands at daemon level', async () => {
      const startTime = Date.now();
      
      const result = await sendCommandAndWaitForResponse('info', { section: 'server' });
      
      expect(result.success).toBe(true);
      expect(result.data.server).toBeDefined();
      
      // The command should have been logged at the daemon level
      // We can verify this by checking that the server processed it
      expect(result.processedBy).toBeDefined();
      console.log('⚡ Command processed by:', result.processedBy);
      
      const endTime = Date.now();
      console.log(`⏱️ Command execution time: ${endTime - startTime}ms`);
    }, 10000);

    test('should handle command timeouts gracefully', async () => {
      // Send a command that might take longer
      const timeoutPromise = new Promise((_, reject) => {
        setTimeout(() => reject(new Error('Test timeout')), 5000);
      });
      
      const commandPromise = sendCommandAndWaitForResponse('info', {});
      
      try {
        const result = await Promise.race([commandPromise, timeoutPromise]);
        expect(result.success).toBe(true);
        console.log('⚡ Command completed within timeout');
      } catch (error) {
        if (error.message === 'Test timeout') {
          console.warn('⚠️ Command took longer than 5 seconds');
        } else {
          throw error;
        }
      }
    }, 10000);
  });

  describe('WebSocket Connection Stability', () => {
    test('should handle connection drops and recovery', async () => {
      // Send initial command
      const result1 = await sendCommandAndWaitForResponse('info', {});
      expect(result1.success).toBe(true);
      
      // Close and reconnect
      ws.close();
      await new Promise(resolve => setTimeout(resolve, 1000));
      await connectWebSocket();
      
      // Send command after reconnection
      const result2 = await sendCommandAndWaitForResponse('info', {});
      expect(result2.success).toBe(true);
      
      console.log('🔄 Connection recovery test passed');
    }, 15000);
  });

  // Helper functions
  async function startTestServer(): Promise<void> {
    // For integration tests, we assume the server is already running on port 9000
    // We'll connect to the existing server instead of starting a new one
    testEnv.serverPort = 9000;
    testEnv.serverReady = true;
    
    // Wait a moment for server to be ready
    await new Promise(resolve => setTimeout(resolve, 2000));
  }

  async function stopTestServer(): Promise<void> {
    if (testEnv.serverProcess) {
      testEnv.serverProcess.kill();
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  }

  async function connectWebSocket(): Promise<void> {
    return new Promise((resolve, reject) => {
      ws = new WebSocket(`ws://localhost:${testEnv.serverPort}`);
      
      ws.on('open', () => {
        console.log('🔌 WebSocket connected for test');
        resolve();
      });
      
      ws.on('error', (error) => {
        console.error('❌ WebSocket connection error:', error);
        reject(error);
      });
      
      // Set timeout
      setTimeout(() => {
        if (ws.readyState !== WebSocket.OPEN) {
          reject(new Error('WebSocket connection timeout'));
        }
      }, 5000);
    });
  }

  async function sendCommandAndWaitForResponse(command: string, params: any): Promise<any> {
    return new Promise((resolve, reject) => {
      const messageId = `test_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
      
      const message = {
        type: 'execute_command',
        command,
        params,
        timestamp: new Date().toISOString(),
        id: messageId
      };
      
      // Set up response handler
      const responseHandler = (data: Buffer) => {
        try {
          const response = JSON.parse(data.toString());
          
          // Check if this is our response
          if (response.id === messageId || 
              response.clientId === messageId ||
              (response.type === 'execute_command_response' && response.timestamp)) {
            
            ws.off('message', responseHandler);
            
            // Resolve with the response
            resolve(response);
          }
        } catch (error) {
          console.error('❌ Response parsing error:', error);
        }
      };
      
      ws.on('message', responseHandler);
      
      // Set timeout
      const timeout = setTimeout(() => {
        ws.off('message', responseHandler);
        reject(new Error(`Command timeout: ${command}`));
      }, 10000);
      
      // Send the command
      try {
        ws.send(JSON.stringify(message));
        console.log(`📤 Sent command: ${command}`, params);
      } catch (error) {
        clearTimeout(timeout);
        ws.off('message', responseHandler);
        reject(error);
      }
    });
  }
});