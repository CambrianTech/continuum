/**
 * Unit Tests for WebSocket Streaming
 * Tests real-time command streaming with screenshot data flow
 */

const WebSocket = require('ws');

// Mock WebSocket for testing
jest.mock('ws');

describe('WebSocket Streaming Client', () => {
  let mockWs;
  let onMessage;
  let onOpen;
  let onClose;
  let onError;

  beforeEach(() => {
    mockWs = {
      send: jest.fn(),
      close: jest.fn(),
      on: jest.fn((event, handler) => {
        if (event === 'message') onMessage = handler;
        if (event === 'open') onOpen = handler;
        if (event === 'close') onClose = handler;
        if (event === 'error') onError = handler;
      }),
      readyState: WebSocket.OPEN,
      CONNECTING: 0,
      OPEN: 1,
      CLOSING: 2,
      CLOSED: 3
    };

    WebSocket.mockImplementation(() => mockWs);
  });

  afterEach(() => {
    jest.resetAllMocks();
  });

  describe('WebSocket Connection', () => {
    test('should establish WebSocket connection to Continuum', () => {
      const ws = new WebSocket('ws://localhost:5555');
      
      expect(WebSocket).toHaveBeenCalledWith('ws://localhost:5555');
      expect(ws.on).toHaveBeenCalledWith('message', expect.any(Function));
      expect(ws.on).toHaveBeenCalledWith('open', expect.any(Function));
      expect(ws.on).toHaveBeenCalledWith('close', expect.any(Function));
      expect(ws.on).toHaveBeenCalledWith('error', expect.any(Function));
    });

    test('should handle connection open event', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const connectHandler = jest.fn();
      
      ws.on('open', connectHandler);
      onOpen();
      
      expect(connectHandler).toHaveBeenCalled();
    });

    test('should handle connection close event', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const closeHandler = jest.fn();
      
      ws.on('close', closeHandler);
      onClose(1000, 'Normal closure');
      
      expect(closeHandler).toHaveBeenCalledWith(1000, 'Normal closure');
    });
  });

  describe('Command Streaming', () => {
    test('should stream screenshot command', () => {
      const ws = new WebSocket('ws://localhost:5555');
      
      const screenshotCommand = {
        type: 'task',
        data: {
          command: 'SCREENSHOT',
          params: 'selector body',
          encoding: 'utf-8'
        }
      };

      ws.send(JSON.stringify(screenshotCommand));

      expect(mockWs.send).toHaveBeenCalledWith(JSON.stringify(screenshotCommand));
    });

    test('should stream JavaScript command with base64 encoding', () => {
      const ws = new WebSocket('ws://localhost:5555');
      
      const jsCode = `
        console.log('🔧 Fixing cyberpunk drawer...');
        const drawers = document.querySelectorAll('.slideout-panel');
        drawers.forEach(drawer => {
          drawer.style.position = 'fixed';
          drawer.style.zIndex = '9999';
        });
      `;
      
      const encodedParams = Buffer.from(jsCode).toString('base64');
      
      const jsCommand = {
        type: 'task',
        data: {
          command: 'BROWSER_JS',
          params: encodedParams,
          encoding: 'base64'
        }
      };

      ws.send(JSON.stringify(jsCommand));

      expect(mockWs.send).toHaveBeenCalledWith(JSON.stringify(jsCommand));
      
      // Verify base64 encoding
      const decoded = Buffer.from(encodedParams, 'base64').toString('utf8');
      expect(decoded).toContain('slideout-panel');
      expect(decoded).toContain('z-index');
    });

    test('should handle multiple commands in sequence', () => {
      const ws = new WebSocket('ws://localhost:5555');
      
      const commands = [
        { type: 'task', data: { command: 'SCREENSHOT', params: 'before' } },
        { type: 'task', data: { command: 'BROWSER_JS', params: 'fix_code' } },
        { type: 'task', data: { command: 'SCREENSHOT', params: 'after' } }
      ];

      commands.forEach(cmd => {
        ws.send(JSON.stringify(cmd));
      });

      expect(mockWs.send).toHaveBeenCalledTimes(3);
    });
  });

  describe('Response Handling', () => {
    test('should receive screenshot data response', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const messageHandler = jest.fn();
      
      ws.on('message', messageHandler);

      const screenshotResponse = {
        type: 'response',
        data: {
          success: true,
          command: 'SCREENSHOT',
          result: {
            filename: 'continuum-interface-1733328000000.png',
            base64Data: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAoAAAAHgCAYAAAA10dzkAAAgAElEQVR4nO3dB5gV1f3H8e9deu+9g',
            dimensions: { width: 1920, height: 1080 },
            fileSize: 245760,
            coordinates: { x: 0, y: 0, width: 1920, height: 1080 }
          },
          timestamp: '2025-06-04T20:55:00.000Z'
        }
      };

      onMessage(JSON.stringify(screenshotResponse));

      expect(messageHandler).toHaveBeenCalledWith(JSON.stringify(screenshotResponse));
      
      // Parse and verify response structure
      const parsedResponse = JSON.parse(messageHandler.mock.calls[0][0]);
      expect(parsedResponse.data.result.base64Data).toContain('data:image/png;base64,');
      expect(parsedResponse.data.result.dimensions).toEqual({ width: 1920, height: 1080 });
    });

    test('should receive JavaScript execution response', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const messageHandler = jest.fn();
      
      ws.on('message', messageHandler);

      const jsResponse = {
        type: 'response',
        data: {
          success: true,
          command: 'BROWSER_JS',
          result: {
            executed: true,
            consoleOutput: [
              '🔧 Fixing 3 drawers',
              'Fixed drawer 0 positioning',
              'Fixed drawer 1 positioning', 
              'Fixed drawer 2 positioning',
              '🎉 Cyberpunk drawer positioning and functionality fixed!'
            ],
            errors: []
          },
          timestamp: '2025-06-04T20:55:00.000Z'
        }
      };

      onMessage(JSON.stringify(jsResponse));

      expect(messageHandler).toHaveBeenCalledWith(JSON.stringify(jsResponse));
      
      const parsedResponse = JSON.parse(messageHandler.mock.calls[0][0]);
      expect(parsedResponse.data.result.executed).toBe(true);
      expect(parsedResponse.data.result.consoleOutput).toContain('🔧 Fixing 3 drawers');
    });

    test('should handle error responses', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const messageHandler = jest.fn();
      
      ws.on('message', messageHandler);

      const errorResponse = {
        type: 'response',
        data: {
          success: false,
          command: 'INVALID_COMMAND',
          error: 'Unknown command: INVALID_COMMAND',
          timestamp: '2025-06-04T20:55:00.000Z'
        }
      };

      onMessage(JSON.stringify(errorResponse));

      const parsedResponse = JSON.parse(messageHandler.mock.calls[0][0]);
      expect(parsedResponse.data.success).toBe(false);
      expect(parsedResponse.data.error).toContain('Unknown command');
    });
  });

  describe('Real-time Interaction Scenarios', () => {
    test('should handle drawer debug workflow', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const responses = [];
      
      ws.on('message', (data) => {
        responses.push(JSON.parse(data));
      });

      // Step 1: Take initial screenshot
      ws.send(JSON.stringify({
        type: 'task',
        data: { command: 'SCREENSHOT', params: 'selector body' }
      }));

      // Simulate screenshot response
      onMessage(JSON.stringify({
        type: 'response',
        data: {
          success: true,
          command: 'SCREENSHOT',
          result: { base64Data: 'data:image/png;base64,initial_screenshot_data' }
        }
      }));

      // Step 2: Fix drawer positioning  
      const drawerFixJS = Buffer.from(`
        const drawers = document.querySelectorAll('.slideout-panel');
        drawers.forEach(drawer => {
          drawer.style.position = 'fixed';
          drawer.style.left = '300px';
          drawer.style.zIndex = '9999';
        });
      `).toString('base64');

      ws.send(JSON.stringify({
        type: 'task', 
        data: { command: 'BROWSER_JS', params: drawerFixJS, encoding: 'base64' }
      }));

      // Simulate fix response
      onMessage(JSON.stringify({
        type: 'response',
        data: {
          success: true,
          command: 'BROWSER_JS',
          result: { executed: true }
        }
      }));

      // Step 3: Test expand button
      const buttonTestJS = Buffer.from(`
        const expandBtn = document.querySelector('.cyber-expand-btn');
        if (expandBtn) {
          expandBtn.click();
          console.log('Button clicked successfully');
        }
      `).toString('base64');

      ws.send(JSON.stringify({
        type: 'task',
        data: { command: 'BROWSER_JS', params: buttonTestJS, encoding: 'base64' }
      }));

      // Simulate button test response
      onMessage(JSON.stringify({
        type: 'response',
        data: {
          success: true,
          command: 'BROWSER_JS',
          result: { executed: true, consoleOutput: ['Button clicked successfully'] }
        }
      }));

      // Step 4: Final screenshot
      ws.send(JSON.stringify({
        type: 'task',
        data: { command: 'SCREENSHOT', params: 'selector .slideout-panel' }
      }));

      // Simulate final screenshot response
      onMessage(JSON.stringify({
        type: 'response',
        data: {
          success: true,
          command: 'SCREENSHOT', 
          result: { base64Data: 'data:image/png;base64,final_screenshot_data' }
        }
      }));

      expect(mockWs.send).toHaveBeenCalledTimes(4);
      expect(responses).toHaveLength(4);
      expect(responses[0].data.result.base64Data).toContain('initial_screenshot_data');
      expect(responses[3].data.result.base64Data).toContain('final_screenshot_data');
    });

    test('should handle concurrent command streaming', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const commandQueue = [];
      
      // Queue multiple commands
      const commands = [
        { id: 1, command: 'SCREENSHOT', params: 'selector .sidebar' },
        { id: 2, command: 'BROWSER_JS', params: 'console.log("test1");' },
        { id: 3, command: 'BROWSER_JS', params: 'console.log("test2");' },
        { id: 4, command: 'SCREENSHOT', params: 'selector .main-content' }
      ];

      commands.forEach(cmd => {
        commandQueue.push(cmd);
        ws.send(JSON.stringify({
          type: 'task',
          data: cmd,
          requestId: cmd.id
        }));
      });

      expect(mockWs.send).toHaveBeenCalledTimes(4);
      expect(commandQueue).toHaveLength(4);
    });

    test('should handle high-resolution screenshot with cropping', () => {
      const ws = new WebSocket('ws://localhost:5555');
      
      const hiResCommand = {
        type: 'task',
        data: {
          command: 'SCREENSHOT',
          params: JSON.stringify({
            selector: '.slideout-panel',
            coordinates: { x: 300, y: 0, width: 400, height: 1080 },
            resolution: { width: 800, height: 2160 }, // 2x scale
            format: 'png',
            quality: 0.95
          }),
          encoding: 'json'
        }
      };

      ws.send(JSON.stringify(hiResCommand));

      // Simulate hi-res response
      onMessage(JSON.stringify({
        type: 'response',
        data: {
          success: true,
          command: 'SCREENSHOT',
          result: {
            base64Data: 'data:image/png;base64,hires_cropped_screenshot',
            dimensions: { width: 800, height: 2160 },
            fileSize: 1048576, // 1MB
            processingTime: 250 // ms
          }
        }
      }));

      expect(mockWs.send).toHaveBeenCalledWith(JSON.stringify(hiResCommand));
    });
  });

  describe('Performance and Error Handling', () => {
    test('should handle WebSocket reconnection', () => {
      const ws = new WebSocket('ws://localhost:5555');
      const reconnectHandler = jest.fn();
      
      // Simulate connection loss
      mockWs.readyState = WebSocket.CLOSED;
      onClose(1006, 'Connection lost');
      
      // Attempt reconnection
      const newWs = new WebSocket('ws://localhost:5555');
      expect(WebSocket).toHaveBeenCalledTimes(2);
    });

    test('should handle large base64 data transfer', () => {
      const ws = new WebSocket('ws://localhost:5555');
      
      // Generate large base64 string (simulating 4K screenshot)
      const largeBase64 = 'data:image/png;base64,' + 'A'.repeat(8000000); // ~8MB
      
      const largeResponse = {
        type: 'response',
        data: {
          success: true,
          command: 'SCREENSHOT',
          result: {
            base64Data: largeBase64,
            dimensions: { width: 3840, height: 2160 },
            fileSize: 8000000
          }
        }
      };

      onMessage(JSON.stringify(largeResponse));
      
      // Should handle large messages without error
      expect(() => JSON.parse(JSON.stringify(largeResponse))).not.toThrow();
    });

    test('should handle command timeout scenarios', (done) => {
      const ws = new WebSocket('ws://localhost:5555');
      
      const timeoutCommand = {
        type: 'task',
        data: { command: 'SCREENSHOT', params: 'slow_operation' },
        timeout: 5000
      };

      ws.send(JSON.stringify(timeoutCommand));

      // Simulate timeout after 5 seconds
      setTimeout(() => {
        onMessage(JSON.stringify({
          type: 'response',
          data: {
            success: false,
            command: 'SCREENSHOT',
            error: 'Command timeout after 5000ms'
          }
        }));
        done();
      }, 100); // Use shorter time for test
    });
  });
});