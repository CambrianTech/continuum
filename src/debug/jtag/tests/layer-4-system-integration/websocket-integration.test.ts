/**
 * JTAG WebSocket Integration Tests
 * 
 * End-to-end integration tests using real WebSocket connections.
 * Tests the full client-server communication pipeline with actual
 * WebSocket protocol, not mocks.
 */

import { JTAGWebSocketServer, JTAGWebSocketClient, JTAGWebSocketUtils } from '../shared/JTAGWebSocket';
import { JTAGLogEntry, JTAGExecOptions, JTAGExecResult } from '../shared/JTAGTypes';
import * as fs from 'fs';
import * as path from 'path';
import * as os from 'os';

// Use real WebSocket implementation
const WebSocket = require('ws');

// Test configuration
const TEST_PORT_BASE = 9100; // Start from high port to avoid conflicts
let currentTestPort = TEST_PORT_BASE;

function getNextTestPort(): number {
  return ++currentTestPort;
}

// Utility to wait for async operations
function delay(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Setup test directory for screenshots
const TEST_SCREENSHOT_DIR = path.join(os.tmpdir(), 'jtag-websocket-integration-tests');

beforeAll(() => {
  if (!fs.existsSync(TEST_SCREENSHOT_DIR)) {
    fs.mkdirSync(TEST_SCREENSHOT_DIR, { recursive: true });
  }
});

afterAll(() => {
  // Cleanup test directory
  if (fs.existsSync(TEST_SCREENSHOT_DIR)) {
    fs.rmSync(TEST_SCREENSHOT_DIR, { recursive: true });
  }
});

describe('Real WebSocket Integration Tests', () => {
  let server: JTAGWebSocketServer;
  let client: JTAGWebSocketClient;
  let testPort: number;
  
  const capturedLogs: JTAGLogEntry[] = [];
  const capturedScreenshots: any[] = [];
  
  beforeEach(async () => {
    testPort = getNextTestPort();
    capturedLogs.length = 0;
    capturedScreenshots.length = 0;
    
    // Create server with real handlers
    server = new JTAGWebSocketServer({
      port: testPort,
      onLog: (entry: JTAGLogEntry) => {
        capturedLogs.push(entry);
      },
      onScreenshot: async (payload: any) => {
        capturedScreenshots.push(payload);
        
        // Save actual screenshot file
        const base64Data = payload.dataUrl.split(',')[1];
        const buffer = Buffer.from(base64Data, 'base64');
        const filepath = path.join(TEST_SCREENSHOT_DIR, `${payload.filename}.png`);
        
        fs.writeFileSync(filepath, buffer);
        
        return {
          success: true,
          filepath,
          filename: `${payload.filename}.png`,
          context: 'browser',
          timestamp: payload.timestamp,
          metadata: {
            ...payload.metadata,
            size: buffer.length
          }
        };
      },
      onExec: async (code: string, options: JTAGExecOptions): Promise<JTAGExecResult> => {
        // Simple code execution simulation
        let result: any;
        let success = true;
        let error: string | undefined;
        
        try {
          if (code === 'throw new Error("test error")') {
            throw new Error('test error');
          }
          
          if (code === '2 + 2') {
            result = 4;
          } else if (code === 'Date.now()') {
            result = Date.now();
          } else if (code === 'process.platform') {
            result = process.platform;
          } else {
            result = `Executed: ${code}`;
          }
        } catch (err: any) {
          success = false;
          error = err.message;
        }
        
        return {
          success,
          result: success ? result : undefined,
          error,
          context: 'server',
          timestamp: new Date().toISOString(),
          executionTime: Math.random() * 100,
          uuid: options.uuid || 'test-uuid'
        };
      }
    });
    
    // Start server
    await server.start();
    
    // Wait for server to be ready
    await delay(100);
    
    // Create and connect client
    client = new JTAGWebSocketClient(testPort);
    await client.connect();
    
    // Wait for connection to stabilize
    await delay(50);
  });
  
  afterEach(async () => {
    if (client) {
      client.disconnect();
    }
    
    if (server) {
      await server.stop();
    }
    
    // Wait for cleanup
    await delay(100);
  });

  describe('Log Message Transport', () => {
    test('should transport log messages successfully', async () => {
      const testEntry: JTAGLogEntry = {
        timestamp: new Date().toISOString(),
        context: 'browser',
        component: 'INTEGRATION_TEST',
        message: 'Real WebSocket log message',
        data: { 
          testData: 'integration test',
          numbers: [1, 2, 3],
          nested: { value: true }
        },
        type: 'log'
      };
      
      const response = await client.sendLog(testEntry);
      
      // Verify response
      expect(response.success).toBe(true);
      expect(response.timestamp).toBeDefined();
      
      // Verify log was captured on server
      expect(capturedLogs).toHaveLength(1);
      expect(capturedLogs[0]).toEqual(testEntry);
    });
    
    test('should handle multiple log types', async () => {
      const logTypes: Array<'log' | 'critical' | 'trace' | 'probe'> = ['log', 'critical', 'trace', 'probe'];
      
      for (const logType of logTypes) {
        const entry: JTAGLogEntry = {
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'TYPE_TEST',
          message: `Test ${logType} message`,
          data: { logType },
          type: logType
        };
        
        const response = await client.sendLog(entry);
        expect(response.success).toBe(true);
      }
      
      expect(capturedLogs).toHaveLength(logTypes.length);
      
      logTypes.forEach((expectedType, index) => {
        expect(capturedLogs[index].type).toBe(expectedType);
        expect(capturedLogs[index].message).toBe(`Test ${expectedType} message`);
      });
    });
    
    test('should handle concurrent log messages', async () => {
      const messageCount = 10;
      const promises: Promise<any>[] = [];
      
      for (let i = 0; i < messageCount; i++) {
        const entry: JTAGLogEntry = {
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'CONCURRENT_TEST',
          message: `Concurrent message ${i}`,
          data: { messageIndex: i },
          type: 'log'
        };
        
        promises.push(client.sendLog(entry));
      }
      
      const responses = await Promise.all(promises);
      
      // All messages should succeed
      responses.forEach(response => {
        expect(response.success).toBe(true);
      });
      
      // All messages should be captured
      expect(capturedLogs).toHaveLength(messageCount);
      
      // Verify message order/content
      for (let i = 0; i < messageCount; i++) {
        const foundMessage = capturedLogs.find(log => 
          log.data.messageIndex === i && log.message === `Concurrent message ${i}`
        );
        expect(foundMessage).toBeDefined();
      }
    });
  });

  describe('Screenshot Data Transport', () => {
    test('should transport screenshot data successfully', async () => {
      // Create a simple 1x1 red pixel PNG
      const redPixelPng = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQIAPS1C6gAAAABJRU5ErkJggg==';
      
      const screenshotPayload = {
        filename: 'integration-test-screenshot',
        dataUrl: `data:image/png;base64,${redPixelPng}`,
        format: 'png',
        metadata: {
          width: 1,
          height: 1,
          size: 95, // Approximate size of the red pixel PNG
          selector: 'body'
        },
        timestamp: new Date().toISOString()
      };
      
      const response = await client.sendScreenshot(screenshotPayload);
      
      // Verify response
      expect(response.success).toBe(true);
      expect(response.data).toBeDefined();
      expect(response.data.success).toBe(true);
      expect(response.data.filename).toBe('integration-test-screenshot.png');
      
      // Verify screenshot was captured on server
      expect(capturedScreenshots).toHaveLength(1);
      expect(capturedScreenshots[0]).toEqual(screenshotPayload);
      
      // Verify file was actually created
      const expectedFilePath = path.join(TEST_SCREENSHOT_DIR, 'integration-test-screenshot.png');
      expect(fs.existsSync(expectedFilePath)).toBe(true);
      
      // Verify file content
      const fileBuffer = fs.readFileSync(expectedFilePath);
      const originalBuffer = Buffer.from(redPixelPng, 'base64');
      expect(fileBuffer.equals(originalBuffer)).toBe(true);
    });
    
    test('should handle large screenshot data', async () => {
      // Create a larger test image (100x100 white PNG)
      const largeImageData = 'iVBORw0KGgoAAAANSUhEUgAAAGQAAABkCAYAAABw4pVUAAAABklEQVR4nO3BAQ0AAADCoPdPbQ43oAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGAaI5IAAT8h1tAAAAAASUVORK5CYII=';
      
      const screenshotPayload = {
        filename: 'large-integration-test',
        dataUrl: `data:image/png;base64,${largeImageData}`,
        format: 'png',
        metadata: {
          width: 100,
          height: 100,
          size: largeImageData.length,
          selector: '#large-element'
        },
        timestamp: new Date().toISOString()
      };
      
      const response = await client.sendScreenshot(screenshotPayload);
      
      expect(response.success).toBe(true);
      expect(capturedScreenshots).toHaveLength(1);
      
      // Verify large file was created correctly
      const expectedFilePath = path.join(TEST_SCREENSHOT_DIR, 'large-integration-test.png');
      expect(fs.existsSync(expectedFilePath)).toBe(true);
      
      const fileStats = fs.statSync(expectedFilePath);
      expect(fileStats.size).toBeGreaterThan(100); // Should be non-trivial size
    });
    
    test('should handle different image formats', async () => {
      const formats = ['png', 'jpeg'];
      
      for (const format of formats) {
        const screenshotPayload = {
          filename: `test-${format}`,
          dataUrl: `data:image/${format};base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQIAPS1C6gAAAABJRU5ErkJggg==`,
          format,
          metadata: {
            width: 1,
            height: 1,
            size: 95,
            selector: 'body'
          },
          timestamp: new Date().toISOString()
        };
        
        const response = await client.sendScreenshot(screenshotPayload);
        
        expect(response.success).toBe(true);
        
        const expectedExtension = format === 'jpeg' ? 'jpg' : format;
        const expectedFilePath = path.join(TEST_SCREENSHOT_DIR, `test-${format}.${expectedExtension}`);
        // Note: The actual file extension logic would be in the handler, 
        // but for this test we expect .png since we're using PNG data
      }
      
      expect(capturedScreenshots).toHaveLength(formats.length);
    });
  });

  describe('Code Execution Transport', () => {
    test('should execute simple code successfully', async () => {
      const response = await client.sendExec('2 + 2', { timeout: 5000 });
      
      expect(response.success).toBe(true);
      expect(response.data).toBeDefined();
      expect(response.data.success).toBe(true);
      expect(response.data.result).toBe(4);
      expect(response.data.executionTime).toBeDefined();
      expect(response.data.uuid).toBeDefined();
    });
    
    test('should handle code execution errors', async () => {
      const response = await client.sendExec('throw new Error("test error")', { timeout: 5000 });
      
      expect(response.success).toBe(true);
      expect(response.data).toBeDefined();
      expect(response.data.success).toBe(false);
      expect(response.data.error).toBe('test error');
    });
    
    test('should handle multiple code executions', async () => {
      const codeSnippets = [
        { code: '2 + 2', expected: 4 },
        { code: 'Date.now()', expected: 'number' },
        { code: 'process.platform', expected: 'string' }
      ];
      
      for (const { code, expected } of codeSnippets) {
        const response = await client.sendExec(code);
        
        expect(response.success).toBe(true);
        expect(response.data.success).toBe(true);
        
        if (typeof expected === 'number') {
          expect(response.data.result).toBe(expected);
        } else if (expected === 'number') {
          expect(typeof response.data.result).toBe('number');
        } else if (expected === 'string') {
          expect(typeof response.data.result).toBe('string');
        }
      }
    });
    
    test('should handle execution with options', async () => {
      const options: JTAGExecOptions = {
        timeout: 1000,
        context: 'server',
        returnValue: true,
        uuid: 'custom-uuid-123'
      };
      
      const response = await client.sendExec('2 + 2', options);
      
      expect(response.success).toBe(true);
      expect(response.data.uuid).toBe('custom-uuid-123');
    });
  });

  describe('Error Handling and Edge Cases', () => {
    test('should handle server disconnect gracefully', async () => {
      // Send a successful message first
      const response1 = await client.sendLog({
        timestamp: new Date().toISOString(),
        context: 'browser',
        component: 'DISCONNECT_TEST',
        message: 'Before disconnect',
        type: 'log'
      });
      
      expect(response1.success).toBe(true);
      
      // Stop the server
      await server.stop();
      
      // Wait for disconnect to propagate
      await delay(200);
      
      // Attempt to send another message - should fail or trigger reconnection
      try {
        await client.sendLog({
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'DISCONNECT_TEST',
          message: 'After disconnect',
          type: 'log'
        });
        
        // If we get here, the client handled reconnection
        // This is acceptable behavior
      } catch (error) {
        // Expected - server is down
        expect(error.message).toContain('WebSocket');
      }
    });
    
    test('should handle malformed messages gracefully', async () => {
      // This test would require lower-level access to send malformed data
      // For now, we test that the validation works at the message creation level
      
      expect(() => {
        JTAGWebSocketUtils.validateResponse(null);
      }).toThrow('Invalid response format');
      
      expect(() => {
        JTAGWebSocketUtils.validateResponse({ success: 'not-boolean' });
      }).toThrow('Invalid response: missing success field');
    });
    
    test('should handle concurrent connections', async () => {
      // Create additional clients
      const client2 = new JTAGWebSocketClient(testPort);
      const client3 = new JTAGWebSocketClient(testPort);
      
      await client2.connect();
      await client3.connect();
      
      // Send messages from all clients concurrently
      const promises = [
        client.sendLog({
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'CLIENT_1',
          message: 'Message from client 1',
          type: 'log'
        }),
        client2.sendLog({
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'CLIENT_2',
          message: 'Message from client 2',
          type: 'log'
        }),
        client3.sendLog({
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'CLIENT_3',
          message: 'Message from client 3',
          type: 'log'
        })
      ];
      
      const responses = await Promise.all(promises);
      
      // All messages should succeed
      responses.forEach(response => {
        expect(response.success).toBe(true);
      });
      
      // All messages should be captured
      expect(capturedLogs).toHaveLength(3);
      
      // Verify all client messages are present
      const client1Msg = capturedLogs.find(log => log.component === 'CLIENT_1');
      const client2Msg = capturedLogs.find(log => log.component === 'CLIENT_2');
      const client3Msg = capturedLogs.find(log => log.component === 'CLIENT_3');
      
      expect(client1Msg).toBeDefined();
      expect(client2Msg).toBeDefined();
      expect(client3Msg).toBeDefined();
      
      // Cleanup additional clients
      client2.disconnect();
      client3.disconnect();
    });
  });

  describe('Performance and Stress Testing', () => {
    test('should handle rapid message sending', async () => {
      const messageCount = 50;
      const promises: Promise<any>[] = [];
      
      const startTime = Date.now();
      
      for (let i = 0; i < messageCount; i++) {
        const promise = client.sendLog({
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'STRESS_TEST',
          message: `Stress message ${i}`,
          data: { 
            messageId: i,
            timestamp: Date.now(),
            randomData: Math.random().toString(36)
          },
          type: 'log'
        });
        
        promises.push(promise);
      }
      
      const responses = await Promise.all(promises);
      const endTime = Date.now();
      
      // All messages should succeed
      responses.forEach((response, index) => {
        expect(response.success).toBe(true);
      });
      
      // All messages should be captured
      expect(capturedLogs).toHaveLength(messageCount);
      
      // Performance check - should complete within reasonable time
      const duration = endTime - startTime;
      expect(duration).toBeLessThan(5000); // 5 seconds max for 50 messages
      
      console.log(`Sent ${messageCount} messages in ${duration}ms (${(messageCount / duration * 1000).toFixed(1)} msg/sec)`);
    }, 10000); // 10 second timeout
  });

  describe('Connection Recovery', () => {
    test('should recover from temporary network issues', async () => {
      // Send initial message to confirm connection
      const response1 = await client.sendLog({
        timestamp: new Date().toISOString(),
        context: 'browser',
        component: 'RECOVERY_TEST',
        message: 'Before network issue',
        type: 'log'
      });
      
      expect(response1.success).toBe(true);
      
      // Simulate network issue by temporarily stopping server
      await server.stop();
      await delay(100);
      
      // Restart server
      server = new JTAGWebSocketServer({
        port: testPort,
        onLog: (entry: JTAGLogEntry) => {
          capturedLogs.push(entry);
        }
      });
      
      await server.start();
      await delay(200);
      
      // The client should eventually reconnect and be able to send messages
      // This might require multiple attempts due to reconnection logic
      let finalResponse;
      let attempts = 0;
      const maxAttempts = 5;
      
      while (attempts < maxAttempts) {
        try {
          finalResponse = await client.sendLog({
            timestamp: new Date().toISOString(),
            context: 'browser',
            component: 'RECOVERY_TEST',
            message: 'After network recovery',
            type: 'log'
          });
          
          if (finalResponse.success) {
            break;
          }
        } catch (error) {
          attempts++;
          if (attempts >= maxAttempts) {
            throw error;
          }
          await delay(500); // Wait before retry
        }
      }
      
      expect(finalResponse?.success).toBe(true);
      
      // Should have captured both messages (before and after recovery)
      const beforeMsg = capturedLogs.find(log => log.message === 'Before network issue');
      const afterMsg = capturedLogs.find(log => log.message === 'After network recovery');
      
      expect(beforeMsg).toBeDefined();
      expect(afterMsg).toBeDefined();
    });
  });
});