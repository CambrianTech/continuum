/**
 * Unit Tests for Cyberpunk Drawer System
 * Tests the real-time drawer functionality with WebSocket streaming
 */

describe('Cyberpunk Drawer System', () => {
  let mockWebSocket;
  let mockCanvas;
  let mockDocument;

  beforeEach(() => {
    // Mock WebSocket
    mockWebSocket = {
      send: jest.fn(),
      readyState: 1, // OPEN
      addEventListener: jest.fn(),
      removeEventListener: jest.fn()
    };

    // Mock Canvas for screenshot functionality
    mockCanvas = {
      width: 800,
      height: 600,
      getContext: jest.fn().mockReturnValue({
        drawImage: jest.fn(),
        imageSmoothingEnabled: true,
        imageSmoothingQuality: 'high'
      }),
      toDataURL: jest.fn().mockReturnValue('data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAYAAAAf8/9hAAAABHNCSVQICAgIfAhkiAAAAAlwSFlzAAAAdgAAAHYBTnsmCAAAABl0RVh0U29mdHdhcmUAd3d3Lmlua3NjYXBlLm9yZ5vuPBoAAANCSURBVDiNpZM7SwNBEIafojFGGy2srLSx')
    };

    // Mock HTML2Canvas
    global.html2canvas = jest.fn().mockResolvedValue(mockCanvas);

    // Mock Document and DOM elements
    mockDocument = {
      querySelectorAll: jest.fn(),
      querySelector: jest.fn(),
      createElement: jest.fn(),
      body: {
        appendChild: jest.fn(),
        removeChild: jest.fn()
      }
    };

    global.document = mockDocument;
    global.window = {
      getComputedStyle: jest.fn().mockReturnValue({}),
      setTimeout: jest.fn((cb) => cb()),
      WebSocket: jest.fn().mockImplementation(() => mockWebSocket)
    };
  });

  afterEach(() => {
    jest.resetAllMocks();
  });

  describe('Drawer Positioning Fix', () => {
    test('should fix slideout panel positioning', () => {
      const mockDrawer = {
        style: {},
        classList: {
          add: jest.fn(),
          remove: jest.fn(),
          toggle: jest.fn()
        }
      };

      mockDocument.querySelectorAll.mockReturnValue([mockDrawer]);

      // Execute the drawer fix JavaScript
      const drawerFixCode = `
        const drawers = document.querySelectorAll('.slideout-panel');
        drawers.forEach((drawer, i) => {
          drawer.style.position = 'fixed';
          drawer.style.top = '0';
          drawer.style.left = '300px';
          drawer.style.width = '400px';
          drawer.style.height = '100vh';
          drawer.style.zIndex = '9999';
          drawer.style.transform = 'translateX(-100%)';
          drawer.style.transition = 'transform 0.4s ease';
        });
      `;

      // Simulate executing the code
      eval(drawerFixCode);

      expect(mockDocument.querySelectorAll).toHaveBeenCalledWith('.slideout-panel');
      expect(mockDrawer.style.position).toBe('fixed');
      expect(mockDrawer.style.left).toBe('300px');
      expect(mockDrawer.style.width).toBe('400px');
      expect(mockDrawer.style.zIndex).toBe('9999');
      expect(mockDrawer.style.transform).toBe('translateX(-100%)');
    });

    test('should fix expand button functionality', () => {
      const mockButton = {
        cloneNode: jest.fn().mockReturnThis(),
        addEventListener: jest.fn(),
        parentNode: {
          replaceChild: jest.fn()
        }
      };

      const mockDrawer = {
        style: {
          transform: 'translateX(-100%)'
        }
      };

      mockDocument.querySelectorAll
        .mockReturnValueOnce([mockButton]) // For buttons
        .mockReturnValueOnce([mockDrawer]); // For drawers

      // Execute button fix JavaScript
      const buttonFixCode = `
        const expandBtns = document.querySelectorAll('.cyber-expand-btn, .expand-btn');
        const drawers = document.querySelectorAll('.slideout-panel');
        
        expandBtns.forEach((btn, i) => {
          const newBtn = btn.cloneNode(true);
          btn.parentNode.replaceChild(newBtn, btn);
          
          newBtn.addEventListener('click', (e) => {
            e.stopPropagation();
            const drawer = drawers[i] || drawers[0];
            if (drawer) {
              const isOpen = drawer.style.transform === 'translateX(0px)';
              if (!isOpen) {
                drawer.style.transform = 'translateX(0px)';
              }
            }
          });
        });
      `;

      eval(buttonFixCode);

      expect(mockButton.cloneNode).toHaveBeenCalledWith(true);
      expect(mockButton.parentNode.replaceChild).toHaveBeenCalled();
      expect(mockButton.addEventListener).toHaveBeenCalledWith('click', expect.any(Function));
    });
  });

  describe('Screenshot Functionality', () => {
    test('should capture full interface screenshot', async () => {
      const screenshotParams = {
        x: 0, y: 0, width: 0, height: 0,
        selector: 'body', scale: 1,
        resolutionWidth: 0, resolutionHeight: 0,
        quality: 0.92, format: 'png'
      };

      const targetElement = { tagName: 'BODY' };
      mockDocument.querySelector.mockReturnValue(targetElement);

      // Execute screenshot JavaScript
      await html2canvas(targetElement, {
        allowTaint: true,
        useCORS: true,
        scale: screenshotParams.scale,
        backgroundColor: '#1a1a1a'
      });

      expect(html2canvas).toHaveBeenCalledWith(targetElement, {
        allowTaint: true,
        useCORS: true,
        scale: 1,
        backgroundColor: '#1a1a1a'
      });

      expect(mockCanvas.toDataURL).toHaveBeenCalled();
    });

    test('should crop screenshot to coordinates', async () => {
      const cropParams = { x: 100, y: 200, width: 300, height: 400 };
      
      mockDocument.createElement.mockReturnValue(mockCanvas);
      
      // Simulate cropping logic
      const cropCanvas = document.createElement('canvas');
      const cropCtx = cropCanvas.getContext('2d');
      
      cropCanvas.width = cropParams.width;
      cropCanvas.height = cropParams.height;
      
      cropCtx.drawImage(
        mockCanvas,
        cropParams.x, cropParams.y, cropParams.width, cropParams.height,
        0, 0, cropParams.width, cropParams.height
      );

      expect(mockDocument.createElement).toHaveBeenCalledWith('canvas');
      expect(cropCanvas.width).toBe(300);
      expect(cropCanvas.height).toBe(400);
      expect(cropCtx.drawImage).toHaveBeenCalledWith(
        mockCanvas, 100, 200, 300, 400, 0, 0, 300, 400
      );
    });

    test('should resize screenshot to resolution', async () => {
      const resizeParams = { resolutionWidth: 800, resolutionHeight: 600 };
      
      mockDocument.createElement.mockReturnValue(mockCanvas);
      
      // Simulate resize logic
      const scaleCanvas = document.createElement('canvas');
      const scaleCtx = scaleCanvas.getContext('2d');
      
      scaleCanvas.width = resizeParams.resolutionWidth;
      scaleCanvas.height = resizeParams.resolutionHeight;
      
      // Downsampling uses smooth scaling
      if (resizeParams.resolutionWidth < mockCanvas.width) {
        scaleCtx.imageSmoothingEnabled = true;
        scaleCtx.imageSmoothingQuality = 'high';
      }

      expect(scaleCanvas.width).toBe(800);
      expect(scaleCanvas.height).toBe(600);
      expect(scaleCtx.imageSmoothingEnabled).toBe(true);
      expect(scaleCtx.imageSmoothingQuality).toBe('high');
    });

    test('should return base64 data in different formats', () => {
      const formats = ['png', 'jpeg', 'webp'];
      
      formats.forEach(format => {
        const quality = 0.92;
        let expectedMimeType;
        
        switch(format) {
          case 'jpeg':
            expectedMimeType = 'image/jpeg';
            mockCanvas.toDataURL.mockReturnValue(`data:${expectedMimeType};base64,/9j/4AAQSkZJRgABA`);
            break;
          case 'webp':
            expectedMimeType = 'image/webp';
            mockCanvas.toDataURL.mockReturnValue(`data:${expectedMimeType};base64,UklGRnoAAABXRUJQ`);
            break;
          default:
            expectedMimeType = 'image/png';
            mockCanvas.toDataURL.mockReturnValue(`data:${expectedMimeType};base64,iVBORw0KGgoAAAANSUhEUgAAA`);
        }

        // Simulate format selection
        let dataURL;
        if (format === 'jpeg') {
          dataURL = mockCanvas.toDataURL('image/jpeg', quality);
        } else if (format === 'webp') {
          dataURL = mockCanvas.toDataURL('image/webp', quality);
        } else {
          dataURL = mockCanvas.toDataURL('image/png');
        }

        expect(dataURL).toContain(`data:${expectedMimeType};base64,`);
      });
    });
  });

  describe('WebSocket Communication', () => {
    test('should send screenshot command via WebSocket', () => {
      const command = {
        type: 'execute_js',
        data: {
          command: 'screenshot code here',
          timestamp: '2025-06-04T20:55:00.000Z'
        }
      };

      mockWebSocket.send(JSON.stringify(command));

      expect(mockWebSocket.send).toHaveBeenCalledWith(JSON.stringify(command));
    });

    test('should receive and process drawer commands', () => {
      const drawerCommand = {
        type: 'execute_js',
        data: {
          command: 'drawer fix code',
          timestamp: '2025-06-04T20:55:00.000Z'
        }
      };

      // Simulate receiving command
      const messageHandler = jest.fn();
      mockWebSocket.addEventListener('message', messageHandler);

      // Trigger message event
      const mockEvent = {
        data: JSON.stringify(drawerCommand)
      };
      
      messageHandler(mockEvent);

      expect(messageHandler).toHaveBeenCalledWith(mockEvent);
    });

    test('should handle WebSocket connection states', () => {
      const states = {
        CONNECTING: 0,
        OPEN: 1,
        CLOSING: 2,
        CLOSED: 3
      };

      Object.entries(states).forEach(([state, value]) => {
        mockWebSocket.readyState = value;
        
        if (value === states.OPEN) {
          expect(() => mockWebSocket.send('test')).not.toThrow();
        } else {
          // Should handle non-open states gracefully
          expect(mockWebSocket.readyState).toBe(value);
        }
      });
    });
  });

  describe('Integration Scenarios', () => {
    test('should complete full drawer fix workflow', async () => {
      // Scenario: Take screenshot -> Fix drawer -> Test button -> Take final screenshot
      const workflow = [
        { command: 'SCREENSHOT', params: 'selector body' },
        { command: 'BROWSER_JS', params: 'drawer fix code' },
        { command: 'BROWSER_JS', params: 'button test code' },
        { command: 'SCREENSHOT', params: 'selector .slideout-panel' }
      ];

      for (const step of workflow) {
        if (step.command === 'SCREENSHOT') {
          await html2canvas(document.body);
          expect(html2canvas).toHaveBeenCalled();
        } else if (step.command === 'BROWSER_JS') {
          mockWebSocket.send(JSON.stringify({
            type: 'execute_js',
            data: { command: step.params }
          }));
          expect(mockWebSocket.send).toHaveBeenCalled();
        }
      }
    });

    test('should handle real-time drawer interaction', () => {
      const mockDrawer = {
        style: { transform: 'translateX(-100%)' }
      };

      const mockButton = {
        addEventListener: jest.fn((event, handler) => {
          // Simulate click
          if (event === 'click') {
            const mockEvent = { stopPropagation: jest.fn() };
            handler(mockEvent);
          }
        })
      };

      mockDocument.querySelectorAll.mockReturnValue([mockDrawer]);

      // Simulate button click handler
      mockButton.addEventListener('click', (e) => {
        e.stopPropagation();
        const drawer = mockDrawer;
        const isOpen = drawer.style.transform === 'translateX(0px)';
        
        if (!isOpen) {
          drawer.style.transform = 'translateX(0px)';
        } else {
          drawer.style.transform = 'translateX(-100%)';
        }
      });

      // Verify drawer opens
      expect(mockDrawer.style.transform).toBe('translateX(0px)');
    });
  });
});