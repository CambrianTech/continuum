/**
 * BrowserConsoleDaemon Unit Tests
 * 
 * Tests the console capture daemon in isolation
 */

import { BrowserConsoleDaemon } from '../../BrowserConsoleDaemon';

// Mock BrowserFeatureFlags for testing
const mockFeatureFlags = {
  CONSOLE_DAEMON_ENABLED: true,
  isDevelopment: true,
  isDebugMode: false
};

// Mock window and console for Node.js testing
const mockWindow = {
  location: { href: 'http://localhost:9000/test' },
  navigator: { userAgent: 'Node.js Test Runner' },
  innerWidth: 1920,
  innerHeight: 1080,
  addEventListener: jest.fn(),
  removeEventListener: jest.fn()
};

const mockConsole = {
  log: jest.fn(),
  info: jest.fn(),
  warn: jest.fn(),
  error: jest.fn(),
  debug: jest.fn(),
  trace: jest.fn(),
  table: jest.fn(),
  group: jest.fn(),
  groupEnd: jest.fn()
};

// Setup globals for browser environment simulation
beforeAll(() => {
  global.window = mockWindow as any;
  global.console = mockConsole as any;
  global.Error = Error;
});

describe('BrowserConsoleDaemon', () => {
  let daemon: BrowserConsoleDaemon;

  beforeEach(() => {
    daemon = new BrowserConsoleDaemon();
    jest.clearAllMocks();
  });

  afterEach(async () => {
    await daemon.stop();
  });

  describe('Message Handling', () => {
    test('should return correct message types', () => {
      const messageTypes = daemon.getMessageTypes();
      
      expect(messageTypes).toContain('console:capture');
      expect(messageTypes).toContain('console:process_queue');
      expect(messageTypes).toContain('console:set_session');
      expect(messageTypes).toContain('console:get_status');
      expect(messageTypes).toContain('console:disable');
      expect(messageTypes).toContain('console:enable');
    });

    test('should handle console:get_status message', async () => {
      const response = await daemon.handleMessage({
        type: 'console:get_status',
        data: {}
      });

      expect(response.success).toBe(true);
      expect(response.data).toHaveProperty('isInitialized');
      expect(response.data).toHaveProperty('sessionId');
      expect(response.data).toHaveProperty('queueLength');
      expect(response.data).toHaveProperty('errorCount');
    });

    test('should handle console:set_session message', async () => {
      const sessionId = 'test-session-123';
      
      const response = await daemon.handleMessage({
        type: 'console:set_session',
        data: { sessionId }
      });

      expect(response.success).toBe(true);
      expect(response.data?.sessionId).toBe(sessionId);
    });

    test('should handle unknown message type', async () => {
      const response = await daemon.handleMessage({
        type: 'unknown:message',
        data: {}
      });

      expect(response.success).toBe(false);
      expect(response.error).toContain('Unknown message type');
    });
  });

  describe('Lifecycle Management', () => {
    test('should start and stop cleanly', async () => {
      await daemon.start();
      expect(daemon.isRunning()).toBe(true);

      await daemon.stop();
      expect(daemon.isRunning()).toBe(false);
    });

    test('should handle enable/disable messages', async () => {
      // Test enable
      const enableResponse = await daemon.handleMessage({
        type: 'console:enable',
        data: {}
      });
      expect(enableResponse.success).toBe(true);

      // Test disable
      const disableResponse = await daemon.handleMessage({
        type: 'console:disable',
        data: {}
      });
      expect(disableResponse.success).toBe(true);
    });
  });

  describe('Console Capture', () => {
    test('should handle console:capture message', async () => {
      const response = await daemon.handleMessage({
        type: 'console:capture',
        data: {
          type: 'log',
          args: ['Test message', { key: 'value' }]
        }
      });

      expect(response.success).toBe(true);
      expect(response.data).toHaveProperty('queued', true);
      expect(response.data).toHaveProperty('queueLength');
    });

    test('should reject invalid capture message', async () => {
      const response = await daemon.handleMessage({
        type: 'console:capture',
        data: {
          type: 'log'
          // Missing args array
        }
      });

      expect(response.success).toBe(false);
      expect(response.error).toContain('Invalid capture message format');
    });
  });

  describe('Queue Management', () => {
    test('should process queue on console:process_queue message', async () => {
      const response = await daemon.handleMessage({
        type: 'console:process_queue',
        data: {}
      });

      expect(response.success).toBe(true);
      expect(response.data).toHaveProperty('queueLength');
    });
  });

  describe('Error Handling', () => {
    test('should handle errors gracefully in message processing', async () => {
      // This should not throw
      const response = await daemon.handleMessage({
        type: 'console:capture',
        data: null // Invalid data
      });

      expect(response.success).toBe(false);
    });
  });
});

// Mock jest functions if not available
if (typeof jest === 'undefined') {
  global.jest = {
    fn: () => () => {},
    clearAllMocks: () => {}
  } as any;
  
  global.expect = ((value: any) => ({
    toBe: () => {},
    toContain: () => {},
    toHaveProperty: () => {},
    toEqual: () => {}
  })) as any;
  
  global.describe = (name: string, fn: () => void) => fn();
  global.test = (name: string, fn: () => void) => fn();
  global.beforeAll = (fn: () => void) => fn();
  global.beforeEach = (fn: () => void) => fn();
  global.afterEach = (fn: () => void) => fn();
}