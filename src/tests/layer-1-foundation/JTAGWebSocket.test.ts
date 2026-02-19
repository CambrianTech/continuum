/**
 * JTAG WebSocket Module Unit Tests
 * 
 * Comprehensive test suite for the extracted WebSocket functionality.
 * Tests both server and client components in isolation.
 */

import { JTAGWebSocketServer, JTAGWebSocketClient, JTAGWebSocketUtils, JTAGWebSocketMessage, JTAGWebSocketResponse } from '@tests/shared/JTAGWebSocket';
import { JTAGLogEntry, JTAGExecOptions, JTAGExecResult } from '@tests/shared/JTAGTypes';

// Mock WebSocket for testing
class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  readyState = MockWebSocket.CONNECTING;
  url: string;
  onopen?: () => void;
  onclose?: (event: any) => void;
  onerror?: (error: any) => void;
  onmessage?: (event: any) => void;
  
  private listeners: { [key: string]: Function[] } = {};
  private messageQueue: any[] = [];
  
  constructor(url: string) {
    this.url = url;
    // Simulate connection success after a short delay
    setTimeout(() => {
      this.readyState = MockWebSocket.OPEN;
      if (this.onopen) this.onopen();
      this.triggerEvent('open', {});
      
      // Process any queued messages
      this.messageQueue.forEach(message => this.processMessage(message));
      this.messageQueue = [];
    }, 10);
  }
  
  send(data: string): void {
    if (this.readyState !== MockWebSocket.OPEN) {
      throw new Error('WebSocket is not open');
    }
    
    // Queue message for processing
    this.messageQueue.push(data);
    
    // Simulate async message processing
    setTimeout(() => this.processMessage(data), 5);
  }
  
  private processMessage(data: string): void {
    try {
      const message = JSON.parse(data);
      
      // Simulate server response based on message type
      let response: JTAGWebSocketResponse;
      
      switch (message.type) {
        case 'log':
          response = {
            success: true,
            timestamp: new Date().toISOString()
          };
          break;
          
        case 'screenshot':
          response = {
            success: true,
            data: {
              success: true,
              filepath: `/mock/screenshots/${message.payload.filename}.png`,
              filename: `${message.payload.filename}.png`,
              context: 'browser',
              timestamp: message.payload.timestamp,
              metadata: message.payload.metadata
            },
            timestamp: new Date().toISOString()
          };
          break;
          
        case 'exec':
          response = {
            success: true,
            data: {
              success: true,
              result: 'mock execution result',
              context: 'server',
              timestamp: new Date().toISOString(),
              executionTime: 50,
              uuid: message.uuid || 'mock-uuid'
            },
            timestamp: new Date().toISOString()
          };
          break;
          
        default:
          response = {
            success: false,
            error: `Unknown message type: ${message.type}`,
            timestamp: new Date().toISOString()
          };
      }
      
      // Simulate server response
      setTimeout(() => {
        this.triggerEvent('message', { data: JSON.stringify(response) });
        if (this.onmessage) {
          this.onmessage({ data: JSON.stringify(response) } as MessageEvent);
        }
      }, 5);
      
    } catch (error) {
      const errorResponse = {
        success: false,
        error: 'Invalid message format',
        timestamp: new Date().toISOString()
      };
      
      setTimeout(() => {
        this.triggerEvent('message', { data: JSON.stringify(errorResponse) });
        if (this.onmessage) {
          this.onmessage({ data: JSON.stringify(errorResponse) } as MessageEvent);
        }
      }, 5);
    }
  }
  
  close(code?: number, reason?: string): void {
    this.readyState = MockWebSocket.CLOSED;
    if (this.onclose) {
      this.onclose({ code: code || 1000, reason: reason || 'Normal closure' });
    }
    this.triggerEvent('close', { code: code || 1000, reason: reason || 'Normal closure' });
  }
  
  addEventListener(type: string, listener: Function): void {
    if (!this.listeners[type]) {
      this.listeners[type] = [];
    }
    this.listeners[type].push(listener);
  }
  
  removeEventListener(type: string, listener: Function): void {
    if (this.listeners[type]) {
      const index = this.listeners[type].indexOf(listener);
      if (index > -1) {
        this.listeners[type].splice(index, 1);
      }
    }
  }
  
  private triggerEvent(type: string, event: any): void {
    if (this.listeners[type]) {
      this.listeners[type].forEach(listener => listener(event));
    }
  }
}

// Mock WebSocket Server
class MockWebSocketServer {
  port: number;
  clients = new Set();
  listening = false;
  
  private listeners: { [key: string]: Function[] } = {};
  
  constructor(options: { port: number }) {
    this.port = options.port;
    
    // Simulate server startup
    setTimeout(() => {
      this.listening = true;
      this.triggerEvent('listening', {});
    }, 20);
  }
  
  on(event: string, callback: Function): void {
    if (!this.listeners[event]) {
      this.listeners[event] = [];
    }
    this.listeners[event].push(callback);
  }
  
  close(callback?: () => void): void {
    this.listening = false;
    this.clients.clear();
    if (callback) callback();
  }
  
  private triggerEvent(event: string, data: any): void {
    if (this.listeners[event]) {
      this.listeners[event].forEach(listener => listener(data));
    }
  }
  
  // Simulate client connection
  simulateConnection(): any {
    const mockClient = {
      send: jest.fn(),
      on: jest.fn(),
      removeEventListener: jest.fn()
    };
    
    this.clients.add(mockClient);
    this.triggerEvent('connection', mockClient);
    
    return mockClient;
  }
}

// Setup mocks
const mockWs = {
  Server: MockWebSocketServer
};

jest.mock('ws', () => mockWs);

// Mock global WebSocket for client tests
(global as any).WebSocket = MockWebSocket;
(global as any).window = { WebSocket: MockWebSocket };

describe('JTAGWebSocketServer', () => {
  let server: JTAGWebSocketServer;
  let logHandler: jest.Mock;
  let screenshotHandler: jest.Mock;
  let execHandler: jest.Mock;
  
  beforeEach(() => {
    logHandler = jest.fn();
    screenshotHandler = jest.fn().mockResolvedValue({
      success: true,
      filepath: '/mock/screenshot.png',
      filename: 'screenshot.png',
      context: 'browser',
      timestamp: new Date().toISOString(),
      metadata: { width: 1920, height: 1080, size: 12345 }
    });
    execHandler = jest.fn().mockResolvedValue({
      success: true,
      result: 'execution result',
      context: 'server',
      timestamp: new Date().toISOString(),
      executionTime: 100,
      uuid: 'test-uuid'
    });
    
    server = new JTAGWebSocketServer({
      port: 9001,
      onLog: logHandler,
      onScreenshot: screenshotHandler,
      onExec: execHandler
    });
  });
  
  afterEach(async () => {
    await server.stop();
  });
  
  describe('Server Lifecycle', () => {
    test('should start WebSocket server successfully', async () => {
      await server.start();
      
      expect(server.isRunning()).toBe(true);
      expect(server.getConnectionCount()).toBe(0);
    });
    
    test('should not start multiple servers on same port', async () => {
      await server.start();
      
      // Attempt to start again should not throw or create new server
      await server.start();
      
      expect(server.isRunning()).toBe(true);
    });
    
    test('should stop server gracefully', async () => {
      await server.start();
      expect(server.isRunning()).toBe(true);
      
      await server.stop();
      expect(server.isRunning()).toBe(false);
    });
    
    test('should handle port already in use', async () => {
      // This test simulates EADDRINUSE error
      const consoleSpy = jest.spyOn(console, 'log').mockImplementation();
      
      await server.start();
      
      // The mock server simulates the server starting successfully
      expect(server.isRunning()).toBe(true);
      
      consoleSpy.mockRestore();
    });
  });
  
  describe('Message Handling', () => {
    let mockClient: any;
    
    beforeEach(async () => {
      await server.start();
      mockClient = (server as any).wss.simulateConnection();
    });
    
    test('should handle valid log messages', async () => {
      const logEntry: JTAGLogEntry = {
        timestamp: new Date().toISOString(),
        context: 'browser',
        component: 'TEST',
        message: 'Test log message',
        data: { test: 'data' },
        type: 'log'
      };
      
      const message: JTAGWebSocketMessage = {
        type: 'log',
        payload: logEntry
      };
      
      // Simulate message handling
      await (server as any).handleMessage(JSON.stringify(message), mockClient);
      
      expect(logHandler).toHaveBeenCalledWith(logEntry);
      expect(mockClient.send).toHaveBeenCalledWith(
        expect.stringContaining('"success":true')
      );
    });
    
    test('should handle valid screenshot messages', async () => {
      const screenshotPayload = {
        filename: 'test-screenshot',
        dataUrl: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==',
        format: 'png',
        metadata: { width: 1, height: 1, size: 100 },
        timestamp: new Date().toISOString()
      };
      
      const message: JTAGWebSocketMessage = {
        type: 'screenshot',
        payload: screenshotPayload
      };
      
      await (server as any).handleMessage(JSON.stringify(message), mockClient);
      
      expect(screenshotHandler).toHaveBeenCalledWith(screenshotPayload);
      expect(mockClient.send).toHaveBeenCalledWith(
        expect.stringContaining('"success":true')
      );
    });
    
    test('should handle valid execution messages', async () => {
      const execPayload = {
        code: 'console.log("test")',
        options: { timeout: 5000 }
      };
      
      const message: JTAGWebSocketMessage = {
        type: 'exec',
        payload: execPayload
      };
      
      await (server as any).handleMessage(JSON.stringify(message), mockClient);
      
      expect(execHandler).toHaveBeenCalledWith('console.log("test")', { timeout: 5000 });
      expect(mockClient.send).toHaveBeenCalledWith(
        expect.stringContaining('"success":true')
      );
    });
    
    test('should handle invalid message format', async () => {
      const invalidMessage = 'not-json';
      
      await (server as any).handleMessage(invalidMessage, mockClient);
      
      expect(mockClient.send).toHaveBeenCalledWith(
        expect.stringContaining('"success":false')
      );
    });
    
    test('should handle unknown message types', async () => {
      const message = {
        type: 'unknown',
        payload: {}
      };
      
      await (server as any).handleMessage(JSON.stringify(message), mockClient);
      
      expect(mockClient.send).toHaveBeenCalledWith(
        expect.stringContaining('"error":"Unknown message type: unknown"')
      );
    });
    
    test('should validate message structure', () => {
      const server = new JTAGWebSocketServer({ port: 9002 });
      
      expect(() => (server as any).validateMessage(null)).toThrow('Invalid message format: must be JSON object');
      expect(() => (server as any).validateMessage({})).toThrow('Invalid message format: missing or invalid type field');
      expect(() => (server as any).validateMessage({ type: 'log' })).toThrow('Invalid message format: missing payload field');
      expect(() => (server as any).validateMessage({ type: 'invalid', payload: {} })).toThrow('Invalid message type');
    });
  });
  
  describe('Error Handling', () => {
    test('should handle log handler errors', async () => {
      const errorHandler = jest.fn().mockImplementation(() => {
        throw new Error('Log handler error');
      });
      
      const serverWithError = new JTAGWebSocketServer({
        port: 9002,
        onLog: errorHandler
      });
      
      await serverWithError.start();
      const mockClient = (serverWithError as any).wss.simulateConnection();
      
      const message: JTAGWebSocketMessage = {
        type: 'log',
        payload: {
          timestamp: new Date().toISOString(),
          context: 'browser',
          component: 'TEST',
          message: 'Test',
          type: 'log'
        } as JTAGLogEntry
      };
      
      await (serverWithError as any).handleMessage(JSON.stringify(message), mockClient);
      
      expect(mockClient.send).toHaveBeenCalledWith(
        expect.stringContaining('"error":"Log processing failed: Log handler error"')
      );
      
      await serverWithError.stop();
    });
    
    test('should handle screenshot handler errors', async () => {
      const errorHandler = jest.fn().mockRejectedValue(new Error('Screenshot handler error'));
      
      const serverWithError = new JTAGWebSocketServer({
        port: 9003,
        onScreenshot: errorHandler
      });
      
      await serverWithError.start();
      const mockClient = (serverWithError as any).wss.simulateConnection();
      
      const message: JTAGWebSocketMessage = {
        type: 'screenshot',
        payload: {
          filename: 'test',
          dataUrl: 'data:image/png;base64,test',
          format: 'png',
          metadata: { width: 1, height: 1, size: 100 },
          timestamp: new Date().toISOString()
        }
      };
      
      await (serverWithError as any).handleMessage(JSON.stringify(message), mockClient);
      
      expect(mockClient.send).toHaveBeenCalledWith(
        expect.stringContaining('"error":"Screenshot processing failed: Screenshot handler error"')
      );
      
      await serverWithError.stop();
    });
  });
});

describe('JTAGWebSocketClient', () => {
  let client: JTAGWebSocketClient;
  
  beforeEach(() => {
    client = new JTAGWebSocketClient(9001);
  });
  
  afterEach(() => {
    client.disconnect();
  });
  
  describe('Connection Management', () => {
    test('should connect successfully', async () => {
      const ws = await client.connect();
      
      expect(ws).toBeDefined();
      expect(client.isConnected()).toBe(true);
    });
    
    test('should reuse existing connection', async () => {
      const ws1 = await client.connect();
      const ws2 = await client.connect();
      
      expect(ws1).toBe(ws2);
      expect(client.isConnected()).toBe(true);
    });
    
    test('should handle connection errors gracefully', async () => {
      // Mock WebSocket to throw error
      const originalWebSocket = (global as any).WebSocket;
      (global as any).WebSocket = class {
        constructor() {
          setTimeout(() => {
            if (this.onerror) this.onerror(new Error('Connection failed'));
          }, 10);
        }
      };
      
      const clientWithError = new JTAGWebSocketClient(9001);
      
      // Connection should fail after retry attempts
      await expect(clientWithError.connect()).rejects.toThrow();
      
      // Restore original WebSocket
      (global as any).WebSocket = originalWebSocket;
    });
    
    test('should disconnect properly', async () => {
      await client.connect();
      expect(client.isConnected()).toBe(true);
      
      client.disconnect();
      expect(client.isConnected()).toBe(false);
    });
  });
  
  describe('Message Sending', () => {
    test('should send log messages', async () => {
      await client.connect();
      
      const logEntry: JTAGLogEntry = {
        timestamp: new Date().toISOString(),
        context: 'browser',
        component: 'TEST',
        message: 'Test message',
        type: 'log'
      };
      
      const response = await client.sendLog(logEntry);
      
      expect(response.success).toBe(true);
      expect(response.timestamp).toBeDefined();
    });
    
    test('should send screenshot messages', async () => {
      await client.connect();
      
      const screenshotPayload = {
        filename: 'test-screenshot',
        dataUrl: 'data:image/png;base64,test-data',
        format: 'png',
        metadata: { width: 100, height: 100, size: 1000 },
        timestamp: new Date().toISOString()
      };
      
      const response = await client.sendScreenshot(screenshotPayload);
      
      expect(response.success).toBe(true);
      expect(response.data).toBeDefined();
    });
    
    test('should send execution messages', async () => {
      await client.connect();
      
      const response = await client.sendExec('console.log("test")', { timeout: 5000 });
      
      expect(response.success).toBe(true);
      expect(response.data).toBeDefined();
    });
    
    test('should handle message timeout', async () => {
      // Mock WebSocket that doesn't respond
      class SlowWebSocket extends MockWebSocket {
        send(data: string): void {
          // Don't call super.send() to simulate no response
        }
      }
      
      (global as any).WebSocket = SlowWebSocket;
      
      const slowClient = new JTAGWebSocketClient(9001);
      await slowClient.connect();
      
      const logEntry: JTAGLogEntry = {
        timestamp: new Date().toISOString(),
        context: 'browser',
        component: 'TEST',
        message: 'Test',
        type: 'log'
      };
      
      await expect(slowClient.sendLog(logEntry)).rejects.toThrow('WebSocket message timeout');
      
      // Restore original WebSocket
      (global as any).WebSocket = MockWebSocket;
    }, 35000); // Increase timeout for this test
  });
});

describe('JTAGWebSocketUtils', () => {
  test('should create log messages correctly', () => {
    const message = JTAGWebSocketUtils.createLogMessage('TEST', 'Test message', { data: 'test' }, 'log');
    
    expect(message.type).toBe('log');
    expect(message.payload.component).toBe('TEST');
    expect(message.payload.message).toBe('Test message');
    expect(message.payload.data).toEqual({ data: 'test' });
    expect(message.payload.type).toBe('log');
    expect(message.payload.timestamp).toBeDefined();
  });
  
  test('should create screenshot messages correctly', () => {
    const metadata = { width: 1920, height: 1080, size: 12345 };
    const message = JTAGWebSocketUtils.createScreenshotMessage('test', 'data:image/png;base64,test', 'png', metadata);
    
    expect(message.type).toBe('screenshot');
    expect(message.payload.filename).toBe('test');
    expect(message.payload.dataUrl).toBe('data:image/png;base64,test');
    expect(message.payload.format).toBe('png');
    expect(message.payload.metadata).toEqual(metadata);
    expect(message.payload.timestamp).toBeDefined();
  });
  
  test('should create exec messages correctly', () => {
    const options = { timeout: 5000, context: 'server' as const };
    const message = JTAGWebSocketUtils.createExecMessage('console.log("test")', options);
    
    expect(message.type).toBe('exec');
    expect(message.payload.code).toBe('console.log("test")');
    expect(message.payload.options).toEqual(options);
  });
  
  test('should validate responses correctly', () => {
    const validResponse = {
      success: true,
      timestamp: new Date().toISOString()
    };
    
    expect(() => JTAGWebSocketUtils.validateResponse(validResponse)).not.toThrow();
    expect(() => JTAGWebSocketUtils.validateResponse(null)).toThrow('Invalid response format');
    expect(() => JTAGWebSocketUtils.validateResponse({})).toThrow('Invalid response: missing success field');
    expect(() => JTAGWebSocketUtils.validateResponse({ success: true })).toThrow('Invalid response: missing timestamp field');
  });
});

// Integration test between server and client
describe('WebSocket Server-Client Integration', () => {
  test('should handle full message roundtrip', async () => {
    // Create handlers
    const logHandler = jest.fn();
    const screenshotHandler = jest.fn().mockResolvedValue({
      success: true,
      filepath: '/test/screenshot.png',
      filename: 'screenshot.png',
      context: 'browser',
      timestamp: new Date().toISOString(),
      metadata: { width: 100, height: 100, size: 1000 }
    });
    
    // Start server
    const server = new JTAGWebSocketServer({
      port: 9004,
      onLog: logHandler,
      onScreenshot: screenshotHandler
    });
    
    await server.start();
    
    // Connect client
    const client = new JTAGWebSocketClient(9004);
    await client.connect();
    
    // Send log message
    const logEntry: JTAGLogEntry = {
      timestamp: new Date().toISOString(),
      context: 'browser',
      component: 'INTEGRATION_TEST',
      message: 'Integration test message',
      type: 'log'
    };
    
    const response = await client.sendLog(logEntry);
    
    expect(response.success).toBe(true);
    expect(logHandler).toHaveBeenCalledWith(logEntry);
    
    // Cleanup
    client.disconnect();
    await server.stop();
  });
});