/**
 * Promise Chain Integration Tests
 * 
 * Tests end-to-end promise preservation across browser ↔ server contexts
 * using the type-safe message system and ResponseCorrelator.
 */

import { JTAGRouter, RouterStatus } from '../../shared/JTAGRouter';
import { ResponseCorrelator } from '../../shared/ResponseCorrelator';
import { 
  JTAGMessageFactory, 
  JTAGMessageTypes, 
  JTAGContext, 
  ScreenshotParams,
  ScreenshotResult,
  JTAGMessage
} from '../../shared/JTAGTypes';

// Mock Transport for Integration Testing
class MockIntegrationTransport {
  name = 'mock-integration-transport';
  private messageHandler?: (message: JTAGMessage) => void;
  private connectedState = true;
  private sentMessages: JTAGMessage[] = [];

  async send(message: JTAGMessage): Promise<any> {
    this.sentMessages.push(message);
    
    // Simulate async transport delay
    await new Promise(resolve => setTimeout(resolve, 10));
    
    // Forward to message handler if set (simulates cross-context delivery)
    if (this.messageHandler) {
      this.messageHandler(message);
    }
    
    return { success: true, transported: true };
  }

  setMessageHandler(handler: (message: JTAGMessage) => void): void {
    this.messageHandler = handler;
  }

  isConnected(): boolean {
    return this.connectedState;
  }

  setConnected(connected: boolean): void {
    this.connectedState = connected;
  }

  async disconnect(): Promise<void> {
    this.connectedState = false;
  }

  getSentMessages(): JTAGMessage[] {
    return [...this.sentMessages];
  }

  clearSentMessages(): void {
    this.sentMessages = [];
  }
}

// Mock Command Handler (simulates screenshot command)
class MockScreenshotHandler {
  async handleMessage(message: JTAGMessage): Promise<ScreenshotResult> {
    if (!JTAGMessageTypes.isRequest(message)) {
      throw new Error('Expected request message');
    }

    const params = message.payload as ScreenshotParams;
    
    // Simulate screenshot processing delay
    await new Promise(resolve => setTimeout(resolve, 50));
    
    return new ScreenshotResult({
      success: true,
      filename: params.filename,
      filepath: `/screenshots/${params.filename}`,
      context: 'server',
      timestamp: new Date().toISOString(),
      metadata: {
        width: 1920,
        height: 1080,
        size: 12345
      }
    });
  }
}

describe('Promise Chain Integration Tests', () => {
  let serverRouter: JTAGRouter;
  let browserRouter: JTAGRouter;
  let serverTransport: MockIntegrationTransport;
  let browserTransport: MockIntegrationTransport;
  let screenshotHandler: MockScreenshotHandler;

  const serverContext: JTAGContext = {
    uuid: 'server-test-uuid',
    environment: 'server'
  };

  const browserContext: JTAGContext = {
    uuid: 'browser-test-uuid', 
    environment: 'browser'
  };

  beforeEach(() => {
    // Create mock transports
    serverTransport = new MockIntegrationTransport();
    browserTransport = new MockIntegrationTransport();
    
    // Create routers with mock transports
    serverRouter = new JTAGRouter(serverContext);
    browserRouter = new JTAGRouter(browserContext);
    
    // Override transports (bypassing normal initialization)
    (serverRouter as any).crossContextTransport = serverTransport;
    (browserRouter as any).crossContextTransport = browserTransport;
    (serverRouter as any).isInitialized = true;
    (browserRouter as any).isInitialized = true;

    // Create screenshot handler
    screenshotHandler = new MockScreenshotHandler();
    
    // Register screenshot handler on server
    serverRouter.registerSubscriber('commands/screenshot', screenshotHandler);

    // Setup cross-context message forwarding
    serverTransport.setMessageHandler((message) => {
      // Server sends to browser
      browserRouter.postMessage(message).catch(console.error);
    });

    browserTransport.setMessageHandler((message) => {
      // Browser sends to server  
      serverRouter.postMessage(message).catch(console.error);
    });
  });

  afterEach(() => {
    serverTransport.clearSentMessages();
    browserTransport.clearSentMessages();
  });

  describe('Request-Response Promise Chain', () => {
    test('should preserve promise chain for cross-context command', async () => {
      // Create screenshot request from browser to server
      const screenshotParams = new ScreenshotParams('test-screenshot.png');
      const correlationId = JTAGMessageFactory.generateCorrelationId();
      
      const requestMessage = JTAGMessageFactory.createRequest(
        browserContext,
        'browser/commands',
        'server/commands/screenshot',
        screenshotParams,
        correlationId
      );

      // Execute request through browser router (simulates browser → server)
      const resultPromise = browserRouter.postMessage(requestMessage);
      
      // Should resolve with actual ScreenshotResult
      const result = await resultPromise;
      
      expect(result).toBeInstanceOf(ScreenshotResult);
      expect(result.success).toBe(true);
      expect(result.filename).toBe('test-screenshot.png');
      expect(result.filepath).toBe('/screenshots/test-screenshot.png');
      expect(result.context).toBe('server');
      expect(result.metadata).toEqual({
        width: 1920,
        height: 1080,
        size: 12345
      });
    });

    test('should handle multiple concurrent requests independently', async () => {
      const requests = [
        new ScreenshotParams('screenshot1.png'),
        new ScreenshotParams('screenshot2.png'), 
        new ScreenshotParams('screenshot3.png')
      ];

      // Create concurrent requests
      const promises = requests.map(params => {
        const correlationId = JTAGMessageFactory.generateCorrelationId();
        const requestMessage = JTAGMessageFactory.createRequest(
          browserContext,
          'browser/commands',
          'server/commands/screenshot',
          params,
          correlationId
        );
        return browserRouter.postMessage(requestMessage);
      });

      // All should resolve with their specific results
      const results = await Promise.all(promises);
      
      expect(results).toHaveLength(3);
      results.forEach((result, index) => {
        expect(result).toBeInstanceOf(ScreenshotResult);
        expect(result.success).toBe(true);
        expect(result.filename).toBe(`screenshot${index + 1}.png`);
        expect(result.filepath).toBe(`/screenshots/screenshot${index + 1}.png`);
      });
    });

    test('should handle request timeout properly', async () => {
      // Create router with very short timeout
      const shortTimeoutRouter = new JTAGRouter(browserContext);
      (shortTimeoutRouter as any).responseCorrelator = new ResponseCorrelator(100); // 100ms timeout
      (shortTimeoutRouter as any).crossContextTransport = browserTransport;
      (shortTimeoutRouter as any).isInitialized = true;

      // Setup transport that doesn't respond
      const nonResponsiveTransport = new MockIntegrationTransport();
      nonResponsiveTransport.setMessageHandler(() => {
        // Do nothing - simulates lost message
      });
      (shortTimeoutRouter as any).crossContextTransport = nonResponsiveTransport;

      const correlationId = JTAGMessageFactory.generateCorrelationId();
      const requestMessage = JTAGMessageFactory.createRequest(
        browserContext,
        'browser/commands',
        'server/commands/screenshot',
        new ScreenshotParams('timeout-test.png'),
        correlationId
      );

      // Should timeout
      await expect(
        shortTimeoutRouter.postMessage(requestMessage)
      ).rejects.toThrow('Request timeout after 100ms');
    });

    test('should handle transport errors gracefully', async () => {
      // Create transport that always fails
      const failingTransport = new MockIntegrationTransport();
      failingTransport.send = async () => {
        throw new Error('Transport connection failed');
      };
      
      (browserRouter as any).crossContextTransport = failingTransport;

      const correlationId = JTAGMessageFactory.generateCorrelationId();
      const requestMessage = JTAGMessageFactory.createRequest(
        browserContext,
        'browser/commands',
        'server/commands/screenshot',
        new ScreenshotParams('fail-test.png'),
        correlationId
      );

      await expect(
        browserRouter.postMessage(requestMessage)
      ).rejects.toThrow('Transport connection failed');
    });
  });

  describe('Event Message Handling', () => {
    test('should handle fire-and-forget events without blocking', async () => {
      // Create console log event
      const logPayload = new (class extends require('../../shared/JTAGTypes').JTAGPayload {
        level = 'info';
        message = 'Test log message';
        timestamp = new Date().toISOString();
      })();

      const eventMessage = JTAGMessageFactory.createEvent(
        browserContext,
        'browser/console',
        'server/console',
        logPayload
      );

      // Should resolve immediately without waiting for processing
      const result = await browserRouter.postMessage(eventMessage);
      
      expect(result.success).toBe(true);
      // Should indicate it was queued, not that we got a response
      expect(result.queued || result.delivered || result.deduplicated).toBe(true);
    });

    test('should deduplicate identical events', async () => {
      const logPayload = new (class extends require('../../shared/JTAGTypes').JTAGPayload {
        level = 'error';
        message = 'Connection failed';
        timestamp = new Date().toISOString();
      })();

      // Send same event multiple times
      const eventMessage = JTAGMessageFactory.createEvent(
        browserContext,
        'browser/console',
        'server/console', 
        logPayload
      );

      const result1 = await browserRouter.postMessage(eventMessage);
      const result2 = await browserRouter.postMessage(eventMessage);
      const result3 = await browserRouter.postMessage(eventMessage);

      // First should succeed, others should be deduplicated
      expect(result1.success).toBe(true);
      expect(result1.deduplicated).toBeFalsy();
      
      expect(result2.success).toBe(true);
      expect(result2.deduplicated).toBe(true);
      
      expect(result3.success).toBe(true);
      expect(result3.deduplicated).toBe(true);
    });
  });

  describe('Router Status and Health', () => {
    test('should report correct router status', () => {
      const status: RouterStatus = serverRouter.status;
      
      expect(status.environment).toBe('server');
      expect(status.initialized).toBe(true);
      expect(status.subscribers).toBeGreaterThanOrEqual(1); // screenshot handler
      expect(status.transport).toEqual({
        name: 'mock-integration-transport',
        connected: true
      });
      expect(status.queue).toBeDefined();
      expect(status.health).toBeDefined();
    });

    test('should handle transport disconnection', async () => {
      // Disconnect transport
      serverTransport.setConnected(false);
      
      const correlationId = JTAGMessageFactory.generateCorrelationId();
      const requestMessage = JTAGMessageFactory.createRequest(
        browserContext,
        'browser/commands',
        'server/commands/screenshot',
        new ScreenshotParams('disconnect-test.png'),
        correlationId
      );

      // Should still attempt to send but may queue or fail
      const resultPromise = browserRouter.postMessage(requestMessage);
      
      // Depending on health manager, this might queue or fail immediately
      await expect(resultPromise).resolves.toBeDefined();
    });
  });

  describe('Mixed Message Type Workflows', () => {
    test('should handle mixed request and event messages correctly', async () => {
      const correlationId = JTAGMessageFactory.generateCorrelationId();
      
      // Create mixed message types
      const requestMessage = JTAGMessageFactory.createRequest(
        browserContext,
        'browser/commands',
        'server/commands/screenshot',
        new ScreenshotParams('mixed-test.png'),
        correlationId
      );

      const eventMessage = JTAGMessageFactory.createEvent(
        browserContext,
        'browser/console',
        'server/console',
        new (class extends require('../../shared/JTAGTypes').JTAGPayload {
          message = 'Starting screenshot';
        })()
      );

      // Send event first (fire-and-forget)
      const eventResult = await browserRouter.postMessage(eventMessage);
      expect(eventResult.success).toBe(true);

      // Send request (should await response)
      const requestResult = await browserRouter.postMessage(requestMessage);
      expect(requestResult).toBeInstanceOf(ScreenshotResult);
      expect(requestResult.success).toBe(true);
    });

    test('should maintain type safety throughout message lifecycle', async () => {
      const correlationId = JTAGMessageFactory.generateCorrelationId();
      const params = new ScreenshotParams('type-safety-test.png');
      
      const requestMessage = JTAGMessageFactory.createRequest(
        browserContext,
        'browser/commands',
        'server/commands/screenshot',
        params,
        correlationId
      );

      // Type guards should work correctly
      expect(JTAGMessageTypes.isRequest(requestMessage)).toBe(true);
      expect(JTAGMessageTypes.isEvent(requestMessage)).toBe(false);
      expect(JTAGMessageTypes.isResponse(requestMessage)).toBe(false);

      // Should preserve payload type
      expect(requestMessage.payload).toBeInstanceOf(ScreenshotParams);
      expect(requestMessage.payload.filename).toBe('type-safety-test.png');

      // Execute and verify response type
      const result = await browserRouter.postMessage(requestMessage);
      expect(result).toBeInstanceOf(ScreenshotResult);
      expect(result.filename).toBe('type-safety-test.png');
    });
  });
});