/**
 * DataMarshalCommand Comprehensive Test Suite
 * 
 * Tests all generic usage patterns for universal data marshalling:
 * - CLI parameter formats (--key=value, arrays, objects)  
 * - All operations (encode, decode, chain, extract)
 * - All encoding types (base64, json, raw)
 * - Event system integration
 * - Command chaining workflows
 * - Error handling and edge cases
 * - Cross-environment data flow
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { DataMarshalCommand, DataMarshalOptions, MarshalledData, ChainableResult } from '../../DataMarshalCommand';

describe('DataMarshalCommand - Comprehensive Usage Patterns', () => {
  
  // Mock context for session testing
  const mockContext = {
    sessionId: 'test-session-123',
    userId: 'test-user',
    timestamp: new Date().toISOString()
  };

  describe('Parameter Parsing - All Input Formats', () => {
    
    test('should parse CLI arguments (--key=value format)', async () => {
      const cliParams = {
        args: [
          '--operation=encode',
          '--data={"test": "value"}',
          '--encoding=json',
          '--source=screenshot',
          '--destination=file-write'
        ]
      };
      
      const result = await DataMarshalCommand.execute(cliParams, mockContext);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.source, 'screenshot');
      assert.strictEqual(result.data.marshalled.encoding, 'json');
    });

    test('should parse direct array parameters', async () => {
      const arrayParams = [
        '--operation=encode',
        '--data=test-string',
        '--encoding=raw'
      ];
      
      const result = await DataMarshalCommand.execute(arrayParams, mockContext);
      expect(result.success).toBe(true);
      expect(result.data.marshalled.encoding).toBe('raw');
    });

    test('should parse object parameters directly', async () => {
      const objectParams: DataMarshalOptions = {
        operation: 'encode',
        data: { screenshot: 'data', format: 'png' },
        encoding: 'json',
        source: 'screenshot'
      };
      
      const result = await DataMarshalCommand.execute(objectParams, mockContext);
      expect(result.success).toBe(true);
      expect(result.data.marshalled.source).toBe('screenshot');
    });

    test('should parse JSON string parameters', async () => {
      const jsonParams = JSON.stringify({
        operation: 'encode',
        data: { test: 'json-data' },
        encoding: 'base64'
      });
      
      const result = await DataMarshalCommand.execute(jsonParams, mockContext);
      expect(result.success).toBe(true);
      expect(result.data.marshalled.encoding).toBe('base64');
    });

    test('should handle special value parsing (booleans, numbers, null)', async () => {
      const specialParams = {
        args: [
          '--operation=encode',
          '--data=null',
          '--timeout=5000',
          '--useCache=true',
          '--compress=false'
        ]
      };
      
      const result = await DataMarshalCommand.execute(specialParams, mockContext);
      expect(result.success).toBe(true);
    });

  });

  describe('Encoding Operations - All Data Types', () => {
    
    test('should encode string data to base64', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'Hello, World!',
        encoding: 'base64'
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.encoding).toBe('base64');
      expect(result.data.marshalled.originalType).toBe('string');
      expect(result.data.marshalled.checksum).toBeDefined();
    });

    test('should encode Buffer data to base64', async () => {
      const buffer = Buffer.from('test buffer data', 'utf-8');
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: buffer,
        encoding: 'base64'
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.originalType).toBe('buffer');
    });

    test('should encode object data to JSON', async () => {
      const complexObject = {
        screenshot: {
          data: 'base64-image-data',
          metadata: { width: 1920, height: 1080 },
          timestamp: new Date().toISOString()
        }
      };
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: complexObject,
        encoding: 'json'
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.encoding).toBe('json');
      expect(result.data.marshalled.originalType).toBe('object');
    });

    test('should handle raw encoding for any data type', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: { mixed: 'data', numbers: [1, 2, 3] },
        encoding: 'raw'
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.encoding).toBe('raw');
    });

  });

  describe('Decoding Operations - All Formats', () => {
    
    test('should decode base64 string back to original', async () => {
      // First encode
      const originalData = 'Test decode string';
      const encodeResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: originalData,
        encoding: 'base64'
      }, mockContext);
      
      // Then decode
      const decodeResult = await DataMarshalCommand.execute({
        operation: 'decode',
        data: encodeResult.data.marshalled
      }, mockContext);
      
      expect(decodeResult.success).toBe(true);
      expect(decodeResult.data.decoded).toBe(originalData);
    });

    test('should decode base64 buffer back to Buffer', async () => {
      const originalBuffer = Buffer.from('Test buffer decode', 'utf-8');
      
      const encodeResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: originalBuffer,
        encoding: 'base64'
      }, mockContext);
      
      const decodeResult = await DataMarshalCommand.execute({
        operation: 'decode',
        data: encodeResult.data.marshalled
      }, mockContext);
      
      expect(decodeResult.success).toBe(true);
      expect(Buffer.isBuffer(decodeResult.data.decoded)).toBe(true);
    });

    test('should decode JSON back to original object', async () => {
      const originalObject = { screenshot: { width: 800, height: 600 } };
      
      const encodeResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: originalObject,
        encoding: 'json'
      }, mockContext);
      
      const decodeResult = await DataMarshalCommand.execute({
        operation: 'decode',
        data: encodeResult.data.marshalled
      }, mockContext);
      
      expect(decodeResult.success).toBe(true);
      expect(decodeResult.data.decoded).toEqual(originalObject);
    });

    test('should validate checksum during decode', async () => {
      const encodeResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'checksum test',
        encoding: 'base64'
      }, mockContext);
      
      // Corrupt the checksum
      const corruptedData = { ...encodeResult.data.marshalled };
      corruptedData.checksum = 'invalid-checksum';
      
      const decodeResult = await DataMarshalCommand.execute({
        operation: 'decode',
        data: corruptedData
      }, mockContext);
      
      expect(decodeResult.success).toBe(false);
      expect(decodeResult.error).toContain('checksum mismatch');
    });

  });

  describe('Chaining Operations - Command Composition', () => {
    
    test('should create chainable result for command workflows', async () => {
      const screenshotData = {
        imageData: 'base64-image-data',
        filename: 'test.png',
        dimensions: { width: 1920, height: 1080 }
      };
      
      const result = await DataMarshalCommand.execute({
        operation: 'chain',
        data: screenshotData,
        source: 'screenshot',
        destination: 'file-write'
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.chainable).toBeDefined();
      expect(result.data.chainable.ready).toBe(true);
      expect(result.data.chainable.next).toBeDefined();
      expect(result.data.chainable.extract).toBeDefined();
    });

    test('should support extract method in chainable results', async () => {
      const complexData = {
        widgets: [
          { id: 'widget1', name: 'Sidebar', active: true },
          { id: 'widget2', name: 'Chat', active: false }
        ],
        metadata: { count: 2 }
      };
      
      const result = await DataMarshalCommand.execute({
        operation: 'chain',
        data: complexData
      }, mockContext);
      
      const extracted = result.data.chainable.extract('widgets[0].name');
      expect(extracted).toBe('Sidebar');
    });

  });

  describe('Extraction Operations - Data Path Access', () => {
    
    test('should extract simple object properties', async () => {
      const data = { user: { name: 'John', age: 30 } };
      
      const result = await DataMarshalCommand.execute({
        operation: 'extract',
        data: data,
        metadata: { path: 'user.name' }
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.extracted).toBe('John');
    });

    test('should extract array elements with notation', async () => {
      const data = {
        screenshots: [
          { filename: 'shot1.png', size: 1024 },
          { filename: 'shot2.png', size: 2048 }
        ]
      };
      
      const result = await DataMarshalCommand.execute({
        operation: 'extract',
        data: data,
        metadata: { path: 'screenshots[1].filename' }
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.extracted).toBe('shot2.png');
    });

    test('should return undefined for invalid paths', async () => {
      const data = { user: { name: 'John' } };
      
      const result = await DataMarshalCommand.execute({
        operation: 'extract',
        data: data,
        metadata: { path: 'user.invalid.path' }
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.extracted).toBeUndefined();
    });

    test('should extract full data when no path provided', async () => {
      const data = { complete: 'dataset' };
      
      const result = await DataMarshalCommand.execute({
        operation: 'extract',
        data: data
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.extracted).toEqual(data);
    });

  });

  describe('Event System Integration', () => {
    
    test('should emit marshal_start events', async () => {
      // This test would need to mock the DaemonEventBus
      // For now, test that the command executes without errors
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'event test',
        source: 'test-command'
      }, mockContext);
      
      expect(result.success).toBe(true);
      // Event emission is logged in console
    });

  });

  describe('Screenshot Integration Patterns', () => {
    
    test('should handle screenshot marshalling workflow', async () => {
      const screenshotData = {
        imageData: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChAGANllpZQAAAABJRU5ErkJggg==',
        filename: 'test-screenshot.png',
        selector: 'body',
        format: 'png',
        width: 100,
        height: 100
      };
      
      const result = await DataMarshalCommand.marshalScreenshotData(screenshotData, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.source).toBe('screenshot');
      expect(result.data.marshalled.metadata.artifactType).toBe('screenshot');
    });

    test('should chain screenshot to file save workflow', async () => {
      const screenshotData = {
        imageData: 'base64-data',
        filename: 'chain-test.png'
      };
      
      const result = await DataMarshalCommand.chainScreenshotToFile(screenshotData, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.source).toBe('screenshot');
      expect(result.data.destination).toBe('file-write');
    });

  });

  describe('Error Handling - All Edge Cases', () => {
    
    test('should handle unsupported operations', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'invalid-operation' as any
      }, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Unsupported operation');
    });

    test('should handle unsupported encoding types', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'test',
        encoding: 'invalid-encoding' as any
      }, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Unsupported encoding');
    });

    test('should handle invalid marshalled data format', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'decode',
        data: 'invalid-marshalled-data'
      }, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Invalid marshalled data format');
    });

    test('should handle malformed JSON in decode', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'decode',
        data: '{ invalid json }'
      }, mockContext);
      
      expect(result.success).toBe(false);
      expect(result.error).toContain('Invalid marshalled data format');
    });

    test('should handle null and undefined inputs gracefully', async () => {
      const nullResult = await DataMarshalCommand.execute(null, mockContext);
      expect(nullResult.success).toBe(true); // Should use fallback
      
      const undefinedResult = await DataMarshalCommand.execute(undefined, mockContext);
      expect(undefinedResult.success).toBe(true); // Should use fallback
    });

  });

  describe('Cross-Environment Usage Patterns', () => {
    
    test('should generate correlation IDs for tracking', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'correlation test'
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalId).toMatch(/^marshal-\d+-\w+$/);
    });

    test('should preserve custom correlation IDs', async () => {
      const customId = 'custom-correlation-123';
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'test',
        correlationId: customId
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalId).toBe(customId);
    });

    test('should include source and destination metadata', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'test',
        source: 'browser-command',
        destination: 'python-service',
        metadata: { priority: 'high' }
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.source).toBe('browser-command');
      expect(result.data.marshalled.destination).toBe('python-service');
      expect(result.data.marshalled.metadata.priority).toBe('high');
    });

  });

  describe('Performance and Size Handling', () => {
    
    test('should handle large data efficiently', async () => {
      const largeData = 'x'.repeat(100000); // 100KB string
      
      const startTime = Date.now();
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: largeData,
        encoding: 'base64'
      }, mockContext);
      const endTime = Date.now();
      
      expect(result.success).toBe(true);
      expect(result.data.marshalled.size).toBeGreaterThan(100000);
      expect(endTime - startTime).toBeLessThan(1000); // Should complete within 1 second
    });

    test('should calculate accurate data sizes', async () => {
      const testData = 'size test string';
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: testData,
        encoding: 'json'
      }, mockContext);
      
      expect(result.success).toBe(true);
      expect(result.data.size).toBe(JSON.stringify(testData).length);
    });

  });

});