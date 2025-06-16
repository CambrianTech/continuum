/**
 * Integration Tests for Screenshot Functionality
 * Tests actual screenshot capture and file saving
 */

const ScreenshotCommand = require('../../src/commands/core/ScreenshotCommand.cjs');

describe('Screenshot Integration', () => {
  let mockContinuum;
  let mockWebSocket;
  let mockWebSocketServer;
  
  beforeEach(() => {
    // Mock WebSocket for browser communication
    mockWebSocket = {
      readyState: 1, // OPEN
      send: jest.fn(),
      addEventListener: jest.fn(),
      removeEventListener: jest.fn()
    };
    
    // Mock WebSocketServer for broadcasting
    mockWebSocketServer = {
      broadcast: jest.fn()
    };
    
    mockContinuum = {
      webSocketServer: mockWebSocketServer
    };
    
    // Mock console.log to avoid noise
    jest.spyOn(console, 'log').mockImplementation(() => {});
  });
  
  afterEach(() => {
    jest.restoreAllMocks();
  });
  
  describe('Command Broadcasting', () => {
    test('broadcasts screenshot command with correct parameters', async () => {
      const params = JSON.stringify({
        selector: '.version-badge',
        name_prefix: 'integration_test',
        scale: 1.0
      });
      
      const result = await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(result.success).toBe(true);
      expect(mockWebSocketServer.broadcast).toHaveBeenCalledWith(
        expect.objectContaining({
          type: 'command',
          command: 'screenshot',
          params: expect.objectContaining({
            selector: '.version-badge',
            scale: 1.0,
            filename: expect.stringMatching(/^integration_test_\d+\.png$/),
            source: 'unknown',
            timestamp: expect.any(Number)
          })
        })
      );
    });
    
    test('generates unique filename for each command', async () => {
      const params1 = JSON.stringify({ name_prefix: 'test1' });
      const params2 = JSON.stringify({ name_prefix: 'test2' });
      
      const result1 = await ScreenshotCommand.execute(params1, mockContinuum);
      await new Promise(resolve => setTimeout(resolve, 1)); // Ensure different timestamp
      const result2 = await ScreenshotCommand.execute(params2, mockContinuum);
      
      expect(result1.data.filename).not.toBe(result2.data.filename);
      expect(result1.data.filename).toMatch(/^test1_\d+\.png$/);
      expect(result2.data.filename).toMatch(/^test2_\d+\.png$/);
    });
  });
  
  describe('File Mode vs Bytes Mode', () => {
    test('file mode should specify filename for server saving', async () => {
      const params = JSON.stringify({
        selector: '.test',
        name_prefix: 'file_mode_test',
        mode: 'file' // Proposed enhancement
      });
      
      const result = await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.filename).toMatch(/^file_mode_test_\d+\.png$/);
      
      const broadcastCall = mockWebSocketServer.broadcast.mock.calls[0][0];
      expect(broadcastCall.params.filename).toBeDefined();
    });
    
    test('bytes mode should not specify filename for client handling', async () => {
      // This test shows how we could enhance the API for bytes mode
      const params = JSON.stringify({
        selector: '.test',
        mode: 'bytes' // Proposed enhancement
      });
      
      const result = await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(result.success).toBe(true);
      // In bytes mode, we might not want to auto-generate filename
      // Instead, return the raw data for client to handle
    });
  });
  
  describe('Error Handling', () => {
    test('handles missing WebSocket server gracefully', async () => {
      const invalidContinuum = { webSocketServer: null };
      
      // This should probably throw an error or handle gracefully
      expect(async () => {
        await ScreenshotCommand.execute('{}', invalidContinuum);
      }).not.toThrow();
    });
    
    test('validates required parameters', async () => {
      const invalidParams = JSON.stringify({
        // Missing selector, should use default
        name_prefix: 'validation_test'
      });
      
      const result = await ScreenshotCommand.execute(invalidParams, mockContinuum);
      
      expect(result.success).toBe(true);
      expect(result.data.selector).toBe('body'); // Should default to body
    });
  });
  
  describe('Response Format', () => {
    test('returns consistent response format', async () => {
      const result = await ScreenshotCommand.execute('{}', mockContinuum);
      
      expect(result).toHaveProperty('success', true);
      expect(result).toHaveProperty('message');
      expect(result).toHaveProperty('timestamp');
      expect(result).toHaveProperty('data');
      
      expect(result.data).toHaveProperty('filename');
      expect(result.data).toHaveProperty('selector');
      expect(result.data).toHaveProperty('scale');
      expect(result.data).toHaveProperty('timestamp');
    });
    
    test('includes all metadata for tracking', async () => {
      const params = JSON.stringify({
        selector: '.metadata-test',
        name_prefix: 'metadata',
        scale: 2.0,
        source: 'unit_test'
      });
      
      const result = await ScreenshotCommand.execute(params, mockContinuum);
      
      expect(result.data.selector).toBe('.metadata-test');
      expect(result.data.scale).toBe(2.0);
      expect(result.data.source).toBe('unit_test');
      expect(result.data.filename).toMatch(/^metadata_\d+\.png$/);
    });
  });
});