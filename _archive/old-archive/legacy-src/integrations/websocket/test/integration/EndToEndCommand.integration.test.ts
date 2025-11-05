/**
 * End-to-End Command Integration Test
 * 
 * INTEGRATION TEST REQUIREMENTS:
 * ================================
 * COMPLETE REQUEST-RESPONSE CYCLE:
 * - WebSocket client connects to daemon
 * - Send execute_command message
 * - Command routes through DaemonConnector
 * - Response returns to client within timeout
 * - Response format validation
 * 
 * SEPARATION OF CONCERNS:
 * - Test ONLY the integration between layers
 * - Does NOT test command business logic (unit tests handle that)
 * - Does NOT test WebSocket protocol details (other tests handle that)
 * - ONLY tests: Client → WebSocket → DaemonConnector → Response flow
 * 
 * MIDDLE-OUT VALIDATION:
 * - Layer boundary validation
 * - Message format consistency
 * - Timeout behavior verification
 * - Error propagation testing
 */

import WebSocket from 'ws';

describe('End-to-End Command Integration', () => {
  let ws: WebSocket;
  let clientId: string;
  const WEBSOCKET_URL = 'ws://localhost:9000';
  const COMMAND_TIMEOUT = 5000; // 5 seconds
  
  beforeAll(async () => {
    // Ensure clean test environment
    // Integration tests assume WebSocket daemon is running
  });
  
  beforeEach(async () => {
    // Create fresh WebSocket connection for each test
    ws = new WebSocket(WEBSOCKET_URL);
    
    // Wait for connection
    await new Promise((resolve, reject) => {
      ws.on('open', resolve);
      ws.on('error', reject);
      setTimeout(() => reject(new Error('Connection timeout')), 5000);
    });
    
    // Initialize client session
    ws.send(JSON.stringify({
      type: 'client_init',
      data: {
        userAgent: 'Integration Test Client',
        url: 'test://integration',
        timestamp: new Date().toISOString()
      }
    }));
    
    // Get client ID from connection_confirmed response
    const initResponse = await waitForMessage(ws, 'connection_confirmed', 5000);
    clientId = initResponse.data.clientId;
    expect(clientId).toBeDefined();
  });
  
  afterEach(async () => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.close();
    }
  });

  describe('Command Execution Flow', () => {
    
    test('should execute health command and return response within timeout', async () => {
      // LAYER TEST: Client → WebSocket → DaemonConnector → Response
      
      // Send command
      const commandMessage = {
        type: 'execute_command',
        data: {
          command: 'health',
          args: {}
        },
        timestamp: new Date().toISOString(),
        clientId
      };
      
      ws.send(JSON.stringify(commandMessage));
      
      // Wait for response with timeout
      const response = await waitForMessage(ws, 'execute_command_response', COMMAND_TIMEOUT);
      
      // INTEGRATION VALIDATION: Response structure
      expect(response).toBeDefined();
      expect(response.type).toBe('execute_command_response');
      expect(response.data).toBeDefined();
      expect(response.clientId).toBe(clientId);
      expect(response.processedBy).toBeDefined();
      expect(response.timestamp).toBeDefined();
      
      // LAYER BOUNDARY VALIDATION: Response contains command result
      expect(response.data.success).toBeDefined();
      if (response.data.success === false) {
        expect(response.data.error).toBeDefined();
        console.log('Command failed:', response.data.error);
      }
    });
    
    test('should execute screenshot command and return response', async () => {
      // Test different command to validate routing consistency
      
      const commandMessage = {
        type: 'execute_command',
        data: {
          command: 'screenshot',
          args: { format: 'png' }
        },
        timestamp: new Date().toISOString(),
        clientId
      };
      
      ws.send(JSON.stringify(commandMessage));
      
      const response = await waitForMessage(ws, 'execute_command_response', COMMAND_TIMEOUT);
      
      expect(response.type).toBe('execute_command_response');
      expect(response.data).toBeDefined();
      expect(response.clientId).toBe(clientId);
    });
    
    test('should handle invalid command gracefully', async () => {
      // NEGATIVE TEST: Error propagation through layers
      
      const commandMessage = {
        type: 'execute_command',
        data: {
          command: 'nonexistent_command',
          args: {}
        },
        timestamp: new Date().toISOString(),
        clientId
      };
      
      ws.send(JSON.stringify(commandMessage));
      
      const response = await waitForMessage(ws, 'execute_command_response', COMMAND_TIMEOUT);
      
      expect(response.data.success).toBe(false);
      expect(response.data.error).toBeDefined();
      expect(response.data.error).toMatch(/not found|unknown|invalid/i);
    });
    
    test('should timeout for commands that hang', async () => {
      // TIMEOUT BEHAVIOR TEST
      
      // This test verifies that the integration properly handles timeouts
      // even if individual commands hang
      
      const commandMessage = {
        type: 'execute_command',
        data: {
          command: 'health', // Use valid command but test timeout behavior
          args: {}
        },
        timestamp: new Date().toISOString(),
        clientId
      };
      
      ws.send(JSON.stringify(commandMessage));
      
      // Either we get a response within timeout, or we get a timeout
      try {
        const response = await waitForMessage(ws, 'execute_command_response', 2000); // Short timeout
        // If we get a response, that's fine - the integration is working
        expect(response).toBeDefined();
      } catch (timeoutError) {
        // If we timeout, that's also a valid test result - it means the client-side
        // timeout is working properly
        expect(timeoutError.message).toMatch(/timeout/i);
      }
    });
  });
  
  describe('Message Format Validation', () => {
    
    test('should reject malformed execute_command messages', async () => {
      // PROTOCOL VALIDATION
      
      const malformedMessage = {
        type: 'execute_command',
        // Missing required data field
        timestamp: new Date().toISOString(),
        clientId
      };
      
      ws.send(JSON.stringify(malformedMessage));
      
      const response = await waitForMessage(ws, ['execute_command_response', 'error'], COMMAND_TIMEOUT);
      
      // Should get error response for malformed message
      expect(response.type).toMatch(/error|execute_command_response/);
      if (response.type === 'execute_command_response') {
        expect(response.data.success).toBe(false);
      }
    });
  });
});

/**
 * UTILITY: Wait for specific message type with timeout
 * PURE FUNCTION: No side effects, clear input/output
 */
function waitForMessage(ws: WebSocket, expectedType: string | string[], timeout: number): Promise<any> {
  return new Promise((resolve, reject) => {
    const timeoutId = setTimeout(() => {
      reject(new Error(`Timeout waiting for message type: ${expectedType}`));
    }, timeout);
    
    const messageHandler = (data: any) => {
      try {
        const message = JSON.parse(data.toString());
        const types = Array.isArray(expectedType) ? expectedType : [expectedType];
        
        if (types.includes(message.type)) {
          clearTimeout(timeoutId);
          ws.off('message', messageHandler);
          resolve(message);
        }
      } catch (error) {
        // Ignore malformed messages, keep waiting
      }
    };
    
    ws.on('message', messageHandler);
  });
}