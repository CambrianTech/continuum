/**
 * Unit Tests for ScreenshotCommand
 * Tests the unified screenshot command functionality
 */

const ScreenshotCommand = require('../../src/commands/core/ScreenshotCommand.cjs');

describe('ScreenshotCommand', () => {
  
  describe('getDefinition', () => {
    test('returns correct command definition', () => {
      const definition = ScreenshotCommand.getDefinition();
      
      expect(definition.name).toBe('SCREENSHOT');
      expect(definition.category).toBe('Core');
      expect(definition.icon).toBe('📸');
      expect(definition.description).toBe('Capture browser screenshot');
      expect(definition.params).toBe('{"selector": ".element"}');
    });
  });
  
  describe('parameter parsing and defaults', () => {
    let mockContinuum;
    
    beforeEach(() => {
      mockContinuum = {
        webSocketServer: {
          broadcast: jest.fn()
        }
      };
      
      // Mock console.log to avoid noise in tests
      jest.spyOn(console, 'log').mockImplementation(() => {});
    });
    
    afterEach(() => {
      jest.restoreAllMocks();
    });
    
    test('uses default parameters when none provided', async () => {
      const result = await ScreenshotCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.selector).toBe('body');
      expect(result.data.scale).toBe(1.0);
      expect(result.data.source).toBe('unknown');
      expect(result.data.filename).toMatch(/^screenshot_\d+\.png$/);
    });
    
    test('uses provided parameters', async () => {
      const params = JSON.stringify({
        selector: '.version-badge',
        name_prefix: 'test',
        scale: 2.0,
        source: 'unit_test'
      });
      
      const result = await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(result.data.selector).toBe('.version-badge');
      expect(result.data.scale).toBe(2.0);
      expect(result.data.source).toBe('unit_test');
      expect(result.data.filename).toMatch(/^test_\d+\.png$/);
    });
    
    test('handles invalid JSON gracefully', async () => {
      const result = await ScreenshotCommand.execute('invalid json', mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.selector).toBe('body'); // Falls back to defaults
    });
  });
  
  describe('WebSocket broadcasting', () => {
    let mockContinuum;
    
    beforeEach(() => {
      mockContinuum = {
        webSocketServer: {
          broadcast: jest.fn()
        }
      };
      jest.spyOn(console, 'log').mockImplementation(() => {});
    });
    
    afterEach(() => {
      jest.restoreAllMocks();
    });
    
    test('broadcasts correct message format', async () => {
      const params = JSON.stringify({
        selector: '.test',
        scale: 1.5,
        source: 'test'
      });
      
      await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(mockContinuum.webSocketServer.broadcast).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'command',
          command: 'screenshot',
          params: expect.objectContaining({
            selector: '.test',
            scale: 1.5,
            source: 'test',
            filename: expect.stringMatching(/^screenshot_\d+\.png$/),
            timestamp: expect.any(Number)
          })
        })
      );
    });
    
    test('includes all required parameters in broadcast', async () => {
      await ScreenshotCommand.execute('{}', mockContinuum);
      
      const broadcastCall = mockContinuum.webSocketServer.broadcast.mock.calls[0][0];
      
      expect(broadcastCall.params).toHaveProperty('selector');
      expect(broadcastCall.params).toHaveProperty('scale');
      expect(broadcastCall.params).toHaveProperty('filename');
      expect(broadcastCall.params).toHaveProperty('manual');
      expect(broadcastCall.params).toHaveProperty('source');
      expect(broadcastCall.params).toHaveProperty('timestamp');
    });
  });
  
  describe('filename generation', () => {
    let mockContinuum;
    
    beforeEach(() => {
      mockContinuum = {
        webSocketServer: {
          broadcast: jest.fn()
        }
      };
      jest.spyOn(console, 'log').mockImplementation(() => {});
    });
    
    afterEach(() => {
      jest.restoreAllMocks();
    });
    
    test('generates unique filenames', async () => {
      const result1 = await ScreenshotCommand.execute('{}', mockContinuum);
      
      // Add small delay to ensure different timestamps
      await new Promise(resolve => setTimeout(resolve, 1));
      
      const result2 = await ScreenshotCommand.execute('{}', mockContinuum);
      
      expect(result1.data.filename).not.toBe(result2.data.filename);
    });
    
    test('uses name_prefix in filename', async () => {
      const params = JSON.stringify({ name_prefix: 'unit_test' });
      const result = await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(result.data.filename).toMatch(/^unit_test_\d+\.png$/);
    });
    
    test('timestamp in filename matches data timestamp', async () => {
      const result = await ScreenshotCommand.execute('{}', mockContinuum);
      
      const filenameTimestamp = result.data.filename.match(/screenshot_(\d+)\.png$/)[1];
      expect(parseInt(filenameTimestamp)).toBe(result.data.timestamp);
    });
  });
  
  describe('result format', () => {
    let mockContinuum;
    
    beforeEach(() => {
      mockContinuum = {
        webSocketServer: {
          broadcast: jest.fn()
        }
      };
      jest.spyOn(console, 'log').mockImplementation(() => {});
    });
    
    afterEach(() => {
      jest.restoreAllMocks();
    });
    
    test('returns success result with all data', async () => {
      const result = await ScreenshotCommand.execute('{}', mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.message).toMatch(/Screenshot .+ requested via elegant API/);
      expect(result.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/);
      
      expect(result.data).toHaveProperty('filename');
      expect(result.data).toHaveProperty('selector');
      expect(result.data).toHaveProperty('scale');
      expect(result.data).toHaveProperty('manual');
      expect(result.data).toHaveProperty('source');
      expect(result.data).toHaveProperty('timestamp');
      expect(result.data).toHaveProperty('message');
    });
  });
  
  describe('console logging', () => {
    let mockContinuum;
    let consoleSpy;
    
    beforeEach(() => {
      mockContinuum = {
        webSocketServer: {
          broadcast: jest.fn()
        }
      };
      consoleSpy = jest.spyOn(console, 'log');
    });
    
    afterEach(() => {
      jest.restoreAllMocks();
    });
    
    test('logs command execution', async () => {
      const params = JSON.stringify({
        selector: '.test',
        source: 'unit_test'
      });
      
      await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringMatching(/📸 SCREENSHOT Command: unit_test requesting \.test/)
      );
      
      expect(consoleSpy).toHaveBeenCalledWith(
        expect.stringMatching(/📸 Broadcasting screenshot command: screenshot_\d+\.png/)
      );
    });
  });
  
  describe('inheritance from BaseCommand', () => {
    test('inherits parseParams method', () => {
      const result = ScreenshotCommand.parseParams('{"test": "value"}');
      expect(result).toEqual({ test: 'value' });
    });
    
    test('inherits createSuccessResult method', () => {
      const result = ScreenshotCommand.createSuccessResult({ test: true });
      expect(result.success).toBe(true);
    });
  });
});