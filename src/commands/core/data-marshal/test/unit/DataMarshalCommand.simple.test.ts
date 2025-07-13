/**
 * DataMarshalCommand Simple Test Suite
 * Core functionality verification for all generic usage patterns
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { DataMarshalCommand } from '../../DataMarshalCommand';

describe('DataMarshalCommand - Core Functionality', () => {
  
  const mockContext = {
    sessionId: 'test-session-123',
    userId: 'test-user'
  };

  describe('Parameter Parsing Patterns', () => {
    
    test('CLI arguments parsing', async () => {
      const cliParams = {
        args: ['--operation=encode', '--data={"test": "value"}', '--encoding=json']
      };
      
      const result = await DataMarshalCommand.execute(cliParams, mockContext);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.encoding, 'json');
    });

    test('Object parameters parsing', async () => {
      const objectParams = {
        operation: 'encode',
        data: { test: 'data' },
        encoding: 'json'
      };
      
      const result = await DataMarshalCommand.execute(objectParams, mockContext);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.encoding, 'json');
    });

    test('JSON string parameters parsing', async () => {
      const jsonParams = JSON.stringify({
        operation: 'encode',
        data: { test: 'json-data' },
        encoding: 'base64'
      });
      
      const result = await DataMarshalCommand.execute(jsonParams, mockContext);
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.encoding, 'base64');
    });

  });

  describe('Encoding Operations', () => {
    
    test('Base64 encoding', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'Hello, World!',
        encoding: 'base64'
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.encoding, 'base64');
      assert.strictEqual(result.data.marshalled.originalType, 'string');
      assert.ok(result.data.marshalled.checksum);
    });

    test('JSON encoding', async () => {
      const complexObject = {
        screenshot: { data: 'base64-data', width: 1920, height: 1080 }
      };
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: complexObject,
        encoding: 'json'
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.encoding, 'json');
      assert.strictEqual(result.data.marshalled.originalType, 'object');
    });

    test('Buffer encoding', async () => {
      const buffer = Buffer.from('test buffer', 'utf-8');
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: buffer,
        encoding: 'base64'
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.originalType, 'buffer');
    });

  });

  describe('Decoding Operations', () => {
    
    test('Round-trip encode/decode string', async () => {
      const originalData = 'Test decode string';
      
      const encodeResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: originalData,
        encoding: 'base64'
      }, mockContext);
      
      const decodeResult = await DataMarshalCommand.execute({
        operation: 'decode',
        data: encodeResult.data.marshalled
      }, mockContext);
      
      assert.strictEqual(decodeResult.success, true);
      assert.strictEqual(decodeResult.data.decoded, originalData);
    });

    test('Round-trip encode/decode object', async () => {
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
      
      assert.strictEqual(decodeResult.success, true);
      assert.deepStrictEqual(decodeResult.data.decoded, originalObject);
    });

    test('Checksum validation', async () => {
      const encodeResult = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'checksum test',
        encoding: 'base64'
      }, mockContext);
      
      // Corrupt the checksum
      const corruptedData = { ...encodeResult.data.marshalled };
      corruptedData.checksum = 'invalid';
      
      const decodeResult = await DataMarshalCommand.execute({
        operation: 'decode',
        data: corruptedData
      }, mockContext);
      
      assert.strictEqual(decodeResult.success, false);
      assert.ok(decodeResult.error.includes('checksum mismatch'));
    });

  });

  describe('Chaining Operations', () => {
    
    test('Create chainable result', async () => {
      const data = { screenshot: 'data', format: 'png' };
      
      const result = await DataMarshalCommand.execute({
        operation: 'chain',
        data: data,
        source: 'screenshot',
        destination: 'file-write'
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.ok(result.data.chainable);
      assert.strictEqual(result.data.chainable.ready, true);
      assert.ok(typeof result.data.chainable.next === 'function');
      assert.ok(typeof result.data.chainable.extract === 'function');
    });

    test('Extract from chainable data', async () => {
      const complexData = {
        widgets: [
          { id: 'widget1', name: 'Sidebar' },
          { id: 'widget2', name: 'Chat' }
        ]
      };
      
      const result = await DataMarshalCommand.execute({
        operation: 'chain',
        data: complexData
      }, mockContext);
      
      const extracted = result.data.chainable.extract('widgets[0].name');
      assert.strictEqual(extracted, 'Sidebar');
    });

  });

  describe('Extraction Operations', () => {
    
    test('Extract object properties', async () => {
      const data = { user: { name: 'John', age: 30 } };
      
      const result = await DataMarshalCommand.execute({
        operation: 'extract',
        data: data,
        metadata: { path: 'user.name' }
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.extracted, 'John');
    });

    test('Extract array elements', async () => {
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
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.extracted, 'shot2.png');
    });

    test('Invalid path returns undefined', async () => {
      const data = { user: { name: 'John' } };
      
      const result = await DataMarshalCommand.execute({
        operation: 'extract',
        data: data,
        metadata: { path: 'user.invalid.path' }
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.extracted, undefined);
    });

  });

  describe('Screenshot Integration', () => {
    
    test('Screenshot marshalling workflow', async () => {
      const screenshotData = {
        imageData: 'data:image/png;base64,iVBORw0KGgo=',
        filename: 'test.png',
        selector: 'body',
        format: 'png',
        width: 100,
        height: 100
      };
      
      const result = await DataMarshalCommand.marshalScreenshotData(screenshotData, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.source, 'screenshot');
      assert.strictEqual(result.data.marshalled.metadata.artifactType, 'screenshot');
    });

    test('Screenshot chain to file workflow', async () => {
      const screenshotData = {
        imageData: 'base64-data',
        filename: 'chain-test.png'
      };
      
      const result = await DataMarshalCommand.chainScreenshotToFile(screenshotData, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.source, 'screenshot');
      assert.strictEqual(result.data.destination, 'file-write');
    });

  });

  describe('Error Handling', () => {
    
    test('Unsupported operation', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'invalid-operation'
      }, mockContext);
      
      assert.strictEqual(result.success, false);
      assert.ok(result.error.includes('Unsupported operation'));
    });

    test('Unsupported encoding', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'test',
        encoding: 'invalid-encoding'
      }, mockContext);
      
      assert.strictEqual(result.success, false);
      assert.ok(result.error.includes('Unsupported encoding'));
    });

    test('Invalid marshalled data', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'decode',
        data: 'invalid-data'
      }, mockContext);
      
      assert.strictEqual(result.success, false);
      assert.ok(result.error.includes('Invalid marshalled data format'));
    });

    test('Null input handling', async () => {
      const result = await DataMarshalCommand.execute(null, mockContext);
      assert.strictEqual(result.success, true); // Should use fallback
    });

  });

  describe('Cross-Environment Patterns', () => {
    
    test('Correlation ID generation', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'correlation test'
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.ok(result.data.marshalId.match(/^marshal-\d+-\w+$/));
    });

    test('Custom correlation ID preservation', async () => {
      const customId = 'custom-correlation-123';
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'test',
        correlationId: customId
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalId, customId);
    });

    test('Source and destination metadata', async () => {
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: 'test',
        source: 'browser-command',
        destination: 'python-service',
        metadata: { priority: 'high' }
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.marshalled.source, 'browser-command');
      assert.strictEqual(result.data.marshalled.destination, 'python-service');
      assert.strictEqual(result.data.marshalled.metadata.priority, 'high');
    });

  });

  describe('Performance Patterns', () => {
    
    test('Large data handling', async () => {
      const largeData = 'x'.repeat(10000); // 10KB string
      
      const startTime = Date.now();
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: largeData,
        encoding: 'base64'
      }, mockContext);
      const endTime = Date.now();
      
      assert.strictEqual(result.success, true);
      assert.ok(result.data.marshalled.size > 10000);
      assert.ok(endTime - startTime < 1000); // Should complete quickly
    });

    test('Accurate size calculation', async () => {
      const testData = 'size test string';
      
      const result = await DataMarshalCommand.execute({
        operation: 'encode',
        data: testData,
        encoding: 'json'
      }, mockContext);
      
      assert.strictEqual(result.success, true);
      assert.strictEqual(result.data.size, JSON.stringify(testData).length);
    });

  });

});