/**
 * ContinuumAPI Unit Tests
 * Test the TypeScript browser API thoroughly
 */

import { jest } from '@jest/globals';

// Mock WebSocket for testing
class MockWebSocket {
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;

  public readyState = MockWebSocket.CONNECTING;
  public onopen: ((event: Event) => void) | null = null;
  public onclose: ((event: CloseEvent) => void) | null = null;
  public onmessage: ((event: MessageEvent) => void) | null = null;
  public onerror: ((event: Event) => void) | null = null;
  
  private sentMessages: string[] = [];

  constructor(public url: string) {
    // Simulate async connection
    setTimeout(() => {
      this.readyState = MockWebSocket.OPEN;
      this.onopen?.(new Event('open'));
    }, 10);
  }

  send(data: string): void {
    this.sentMessages.push(data);
  }

  close(): void {
    this.readyState = MockWebSocket.CLOSED;
    this.onclose?.(new CloseEvent('close'));
  }

  // Test helper methods
  getSentMessages(): string[] {
    return [...this.sentMessages];
  }

  simulateMessage(data: any): void {
    const event = new MessageEvent('message', { data: JSON.stringify(data) });
    this.onmessage?.(event);
  }

  simulateError(): void {
    this.onerror?.(new Event('error'));
  }
}

// Mock fetch for version endpoint
const mockFetch = jest.fn();

// Setup global mocks
(global as any).WebSocket = MockWebSocket;
(global as any).fetch = mockFetch;
(global as any).window = {
  dispatchEvent: jest.fn(),
  addEventListener: jest.fn(),
  removeEventListener: jest.fn()
};
(global as any).document = {
  readyState: 'complete',
  addEventListener: jest.fn()
};

// Import the API after mocks are set up
let ContinuumAPI: any;

beforeAll(async () => {
  // Dynamic import to load after mocks
  const fs = await import('fs');
  const path = await import('path');
  
  // Read and evaluate the transpiled continuum.js
  const apiPath = path.join(process.cwd(), 'src/ui/continuum.js');
  const apiCode = fs.readFileSync(apiPath, 'utf8');
  
  // Create a safe evaluation context
  const context = {
    window: (global as any).window,
    document: (global as any).document,
    WebSocket: MockWebSocket,
    fetch: mockFetch,
    console,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
    Promise,
    Error,
    Map,
    JSON,
    Date,
    Math,
    CustomEvent: class CustomEvent {
      constructor(public type: string, public detail?: any) {}
    }
  };
  
  // Execute the API code in our context
  const evalCode = `
    ${apiCode}
    return window.continuum;
  `;
  
  const func = new Function(...Object.keys(context), evalCode);
  ContinuumAPI = func(...Object.values(context));
});

describe('ContinuumAPI', () => {
  let api: any;
  let mockWs: MockWebSocket;

  beforeEach(() => {
    jest.clearAllMocks();
    
    // Reset fetch mock
    mockFetch.mockResolvedValue({
      json: () => Promise.resolve({
        version: '0.2.2204',
        build: 'test',
        server: 'test-server',
        environment: 'test'
      })
    });

    // Create fresh API instance
    api = new (ContinuumAPI.constructor)();
  });

  describe('Initialization', () => {
    test('should create API instance with default state', () => {
      expect(api.isConnected()).toBe(false);
      expect(api.version).toBeUndefined();
    });

    test('should have required methods', () => {
      expect(typeof api.connect).toBe('function');
      expect(typeof api.execute).toBe('function');
      expect(typeof api.info).toBe('function');
      expect(typeof api.chat).toBe('function');
      expect(typeof api.screenshot).toBe('function');
      expect(typeof api.loadChatMessages).toBe('function');
      expect(typeof api.on).toBe('function');
      expect(typeof api.off).toBe('function');
      expect(typeof api.emit).toBe('function');
    });
  });

  describe('Connection Management', () => {
    test('should connect successfully', async () => {
      const connectPromise = api.connect();
      
      // Wait for connection
      await connectPromise;
      
      expect(api.isConnected()).toBe(true);
      expect(mockFetch).toHaveBeenCalledWith('/api/version');
      expect(api.version).toBe('0.2.2204');
    });

    test('should handle version fetch failure gracefully', async () => {
      mockFetch.mockRejectedValue(new Error('Network error'));
      
      const connectPromise = api.connect();
      await connectPromise;
      
      expect(api.isConnected()).toBe(true);
      expect(api.version).toBeUndefined();
    });

    test('should handle WebSocket connection error', async () => {
      // Override WebSocket to simulate connection failure
      (global as any).WebSocket = class FailingWebSocket {
        constructor() {
          setTimeout(() => {
            this.onerror?.(new Event('error'));
          }, 10);
        }
        onerror: ((event: Event) => void) | null = null;
      };

      await expect(api.connect()).rejects.toThrow();
    });

    test('should dispatch custom events on connection', async () => {
      const mockDispatchEvent = (global as any).window.dispatchEvent;
      
      await api.connect();
      
      expect(mockDispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'continuum:version-update'
        })
      );
      expect(mockDispatchEvent).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'continuum-connected'
        })
      );
    });
  });

  describe('Command Execution', () => {
    beforeEach(async () => {
      await api.connect();
      mockWs = api.ws as MockWebSocket;
    });

    test('should execute commands with promises', async () => {
      const executePromise = api.execute('test-command', { param: 'value' });
      
      // Check message was sent
      const sentMessages = mockWs.getSentMessages();
      expect(sentMessages).toHaveLength(1);
      
      const sentMessage = JSON.parse(sentMessages[0]);
      expect(sentMessage).toMatchObject({
        type: 'execute_command',
        command: 'test-command',
        params: { param: 'value' }
      });
      expect(sentMessage.id).toMatch(/^cmd_\d+_\w+$/);
      
      // Simulate successful response
      mockWs.simulateMessage({
        id: sentMessage.id,
        success: true,
        data: { result: 'success' }
      });
      
      const result = await executePromise;
      expect(result).toEqual({ result: 'success' });
    });

    test('should handle command failures', async () => {
      const executePromise = api.execute('failing-command');
      
      const sentMessages = mockWs.getSentMessages();
      const sentMessage = JSON.parse(sentMessages[0]);
      
      // Simulate error response
      mockWs.simulateMessage({
        id: sentMessage.id,
        success: false,
        error: 'Command failed'
      });
      
      await expect(executePromise).rejects.toThrow('Command failed');
    });

    test('should reject when not connected', async () => {
      // Create disconnected API
      const disconnectedApi = new (ContinuumAPI.constructor)();
      
      await expect(disconnectedApi.execute('test')).rejects.toThrow('Continuum not connected');
    });

    test('should handle message sending errors', async () => {
      // Mock WebSocket send to throw
      mockWs.send = jest.fn().mockImplementation(() => {
        throw new Error('Send failed');
      });
      
      await expect(api.execute('test')).rejects.toThrow('Send failed');
    });
  });

  describe('Typed Command Methods', () => {
    beforeEach(async () => {
      await api.connect();
      mockWs = api.ws as MockWebSocket;
    });

    test('info() should call execute with correct parameters', async () => {
      const infoPromise = api.info({ section: 'system' });
      
      const sentMessages = mockWs.getSentMessages();
      const sentMessage = JSON.parse(sentMessages[0]);
      
      expect(sentMessage.command).toBe('info');
      expect(sentMessage.params).toEqual({ section: 'system' });
      
      // Simulate response
      mockWs.simulateMessage({
        id: sentMessage.id,
        success: true,
        data: { version: '0.2.2204', system: { platform: 'test' } }
      });
      
      const result = await infoPromise;
      expect(result.version).toBe('0.2.2204');
    });

    test('chat() should call execute with message and room', async () => {
      const chatPromise = api.chat('Hello world', 'test-room');
      
      const sentMessages = mockWs.getSentMessages();
      const sentMessage = JSON.parse(sentMessages[0]);
      
      expect(sentMessage.command).toBe('chat');
      expect(sentMessage.params).toEqual({ 
        message: 'Hello world', 
        room: 'test-room' 
      });
      
      // Simulate response
      mockWs.simulateMessage({
        id: sentMessage.id,
        success: true,
        data: { messageId: 'msg-123' }
      });
      
      await chatPromise;
    });

    test('screenshot() should call execute with screenshot params', async () => {
      const screenshotPromise = api.screenshot({ 
        filename: 'test.png', 
        fullPage: true 
      });
      
      const sentMessages = mockWs.getSentMessages();
      const sentMessage = JSON.parse(sentMessages[0]);
      
      expect(sentMessage.command).toBe('screenshot');
      expect(sentMessage.params).toEqual({ 
        filename: 'test.png', 
        fullPage: true 
      });
      
      // Simulate response
      mockWs.simulateMessage({
        id: sentMessage.id,
        success: true,
        data: { filename: 'test.png', path: '/screenshots/test.png' }
      });
      
      const result = await screenshotPromise;
      expect(result.filename).toBe('test.png');
    });

    test('loadChatMessages() should use default room', async () => {
      const loadPromise = api.loadChatMessages();
      
      const sentMessages = mockWs.getSentMessages();
      const sentMessage = JSON.parse(sentMessages[0]);
      
      expect(sentMessage.command).toBe('load_chat_messages');
      expect(sentMessage.params).toEqual({ room: 'general' });
      
      // Simulate response
      mockWs.simulateMessage({
        id: sentMessage.id,
        success: true,
        data: [{ message: 'Hello', timestamp: '2025-06-28T00:00:00Z' }]
      });
      
      await loadPromise;
    });
  });

  describe('Event Handling', () => {
    test('should register and trigger event handlers', () => {
      const handler1 = jest.fn();
      const handler2 = jest.fn();
      
      api.on('test-event', handler1);
      api.on('test-event', handler2);
      api.on('other-event', jest.fn());
      
      api.emit('test-event', { data: 'test' });
      
      expect(handler1).toHaveBeenCalledWith({ data: 'test' });
      expect(handler2).toHaveBeenCalledWith({ data: 'test' });
    });

    test('should remove event handlers', () => {
      const handler = jest.fn();
      
      api.on('test-event', handler);
      api.emit('test-event', { data: 'test' });
      expect(handler).toHaveBeenCalledTimes(1);
      
      api.off('test-event', handler);
      api.emit('test-event', { data: 'test' });
      expect(handler).toHaveBeenCalledTimes(1); // Should not be called again
    });

    test('should handle event handler errors gracefully', () => {
      const errorHandler = jest.fn().mockImplementation(() => {
        throw new Error('Handler error');
      });
      const goodHandler = jest.fn();
      
      api.on('test-event', errorHandler);
      api.on('test-event', goodHandler);
      
      // Should not throw
      api.emit('test-event', { data: 'test' });
      
      expect(errorHandler).toHaveBeenCalled();
      expect(goodHandler).toHaveBeenCalled();
    });
  });

  describe('Connection Recovery', () => {
    test('should attempt reconnection on close', async () => {
      await api.connect();
      const originalConnect = api.connect;
      api.connect = jest.fn().mockResolvedValue(undefined);
      
      // Simulate connection close
      mockWs.close();
      
      // Wait for reconnection attempt
      await new Promise(resolve => setTimeout(resolve, 3100));
      
      expect(api.connect).toHaveBeenCalled();
    });
  });

  describe('Message Response Handling', () => {
    beforeEach(async () => {
      await api.connect();
      mockWs = api.ws as MockWebSocket;
    });

    test('should handle message responses by ID', async () => {
      const executePromise = api.execute('test');
      
      const sentMessages = mockWs.getSentMessages();
      const sentMessage = JSON.parse(sentMessages[0]);
      
      // Simulate response with correct ID
      mockWs.simulateMessage({
        id: sentMessage.id,
        success: true,
        data: { test: 'response' }
      });
      
      const result = await executePromise;
      expect(result).toEqual({ test: 'response' });
    });

    test('should ignore messages without matching ID', async () => {
      const executePromise = api.execute('test');
      
      // Simulate response with wrong ID
      mockWs.simulateMessage({
        id: 'wrong-id',
        success: true,
        data: { test: 'wrong' }
      });
      
      // Promise should still be pending
      const promiseState = await Promise.race([
        executePromise.then(() => 'resolved'),
        new Promise(resolve => setTimeout(() => resolve('timeout'), 100))
      ]);
      
      expect(promiseState).toBe('timeout');
    });

    test('should handle malformed message responses', async () => {
      await api.connect();
      
      // Simulate malformed JSON
      const mockMessageEvent = new MessageEvent('message', { 
        data: 'invalid-json{' 
      });
      
      // Should not throw
      expect(() => {
        mockWs.onmessage?.(mockMessageEvent);
      }).not.toThrow();
    });
  });
});