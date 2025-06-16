/**
 * FileSave Command Tests
 * Tests file saving functionality with proper directory structure and error handling
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';
import fs from 'fs';
import path from 'path';

const require = createRequire(import.meta.url);
const FileSaveCommand = require('../FileSaveCommand.cjs');

describe('FileSave Command Tests', () => {

  function createTestDir() {
    const testDir = path.join(process.cwd(), '.continuum', 'test-files');
    if (fs.existsSync(testDir)) {
      fs.rmSync(testDir, { recursive: true, force: true });
    }
    fs.mkdirSync(testDir, { recursive: true });
    return testDir;
  }

  function createTestDataURL() {
    // Small 1x1 PNG image as data URL
    return 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
  }

  describe('Basic File Saving', () => {
    test('should save file with data URL format', async () => {
      const testDir = createTestDir();
      const dataURL = createTestDataURL();
      
      const result = await FileSaveCommand.execute(JSON.stringify({
        filename: 'test.png',
        data: dataURL,
        baseDirectory: '.continuum/test-files'
      }));
      
      assert(result.success, 'FileSave should succeed');
      assert.strictEqual(result.data.filename, 'test.png');
      assert(result.data.fileSize > 0, 'File should have content');
      
      const savedFile = path.join(testDir, 'test.png');
      assert(fs.existsSync(savedFile), 'File should exist on filesystem');
      
      const fileContent = fs.readFileSync(savedFile);
      assert(fileContent.length > 0, 'File should have content');
    });

    test('should save file with base64 data', async () => {
      const testDir = createTestDir();
      const base64Data = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
      
      const result = await FileSaveCommand.execute(JSON.stringify({
        filename: 'base64-test.png',
        data: base64Data,
        baseDirectory: '.continuum/test-files'
      }));
      
      assert(result.success, 'FileSave should succeed with base64');
      assert.strictEqual(result.data.filename, 'base64-test.png');
      
      const savedFile = path.join(testDir, 'base64-test.png');
      assert(fs.existsSync(savedFile), 'Base64 file should exist');
    });

    test('should create nested directory structure', async () => {
      const testDir = createTestDir();
      const dataURL = createTestDataURL();
      
      const result = await FileSaveCommand.execute(JSON.stringify({
        filename: 'emotions/love/test-animation.png',
        data: dataURL,
        baseDirectory: '.continuum/test-files'
      }));
      
      assert(result.success, 'FileSave should succeed with nested path');
      assert.strictEqual(result.data.filename, 'emotions/love/test-animation.png');
      assert.strictEqual(result.data.directory, 'emotions/love');
      assert.strictEqual(result.data.actualFilename, 'test-animation.png');
      
      const savedFile = path.join(testDir, 'emotions', 'love', 'test-animation.png');
      assert(fs.existsSync(savedFile), 'Nested file should exist');
      
      const nestedDir = path.join(testDir, 'emotions', 'love');
      assert(fs.existsSync(nestedDir), 'Nested directory should be created');
    });
  });

  describe('MIME Type Detection', () => {
    test('should detect PNG MIME type', () => {
      const mimeType = FileSaveCommand.detectMimeType('test.png');
      assert.strictEqual(mimeType, 'image/png');
    });

    test('should detect JPEG MIME types', () => {
      assert.strictEqual(FileSaveCommand.detectMimeType('test.jpg'), 'image/jpeg');
      assert.strictEqual(FileSaveCommand.detectMimeType('test.jpeg'), 'image/jpeg');
    });

    test('should handle unknown extensions', () => {
      const mimeType = FileSaveCommand.detectMimeType('test.unknown');
      assert.strictEqual(mimeType, 'application/octet-stream');
    });

    test('should be case insensitive', () => {
      assert.strictEqual(FileSaveCommand.detectMimeType('TEST.PNG'), 'image/png');
      assert.strictEqual(FileSaveCommand.detectMimeType('file.JPG'), 'image/jpeg');
    });
  });

  describe('File Size Validation', () => {
    test('should report correct file size', async () => {
      const testDir = createTestDir();
      const dataURL = createTestDataURL();
      
      const result = await FileSaveCommand.execute(JSON.stringify({
        filename: 'size-test.png',
        data: dataURL,
        baseDirectory: '.continuum/test-files'
      }));
      
      assert(result.success);
      assert(result.data.fileSize > 0, 'Should report file size in bytes');
      assert(result.data.fileSizeKB >= 1, 'Should report file size in KB');
      
      const savedFile = path.join(testDir, 'size-test.png');
      const actualSize = fs.statSync(savedFile).size;
      assert.strictEqual(result.data.fileSize, actualSize, 'Reported size should match actual file size');
    });
  });

  describe('Error Handling', () => {
    test('should handle invalid base64 data', async () => {
      const result = await FileSaveCommand.execute(JSON.stringify({
        filename: 'invalid.png',
        data: 'invalid-base64-data',
        baseDirectory: '.continuum/test-files'
      }));
      
      assert(!result.success, 'Should fail with invalid base64');
      assert(result.message.includes('failed') || result.message.includes('error'), 'Should include error message');
    });

    test('should handle malformed JSON parameters', async () => {
      const result = await FileSaveCommand.execute('invalid json');
      
      assert(!result.success, 'Should fail with invalid JSON');
    });

    test('should handle missing required parameters', async () => {
      const result = await FileSaveCommand.execute(JSON.stringify({
        filename: 'test.png'
        // Missing data parameter
      }));
      
      assert(!result.success, 'Should fail with missing data parameter');
    });
  });

  describe('Command Integration Tests', () => {
    test('should work with screenshot command delegation pattern', async () => {
      const testDir = createTestDir();
      const dataURL = createTestDataURL();
      
      // Simulate screenshot -> fileSave delegation
      const screenshotResult = {
        success: true,
        dataURL: dataURL,
        filename: 'screenshot-test.png'
      };
      
      // FileSave handles the actual saving
      const saveResult = await FileSaveCommand.execute(JSON.stringify({
        filename: screenshotResult.filename,
        data: screenshotResult.dataURL,
        baseDirectory: '.continuum/test-files'
      }));
      
      assert(saveResult.success, 'Screenshot delegation should work');
      assert.strictEqual(saveResult.data.filename, 'screenshot-test.png');
      
      const savedFile = path.join(testDir, 'screenshot-test.png');
      assert(fs.existsSync(savedFile), 'Delegated file should be saved');
    });

    test('should support emotion animation screenshot saving', async () => {
      const testDir = createTestDir();
      const dataURL = createTestDataURL();
      
      // Simulate emotion animation screenshot
      const emotionTest = {
        emotion: 'love',
        intensity: 'strong',
        timestamp: new Date().toISOString()
      };
      
      const result = await FileSaveCommand.execute(JSON.stringify({
        filename: `emotions/${emotionTest.emotion}_${emotionTest.intensity}.png`,
        data: dataURL,
        baseDirectory: '.continuum/test-files',
        mimeType: 'image/png'
      }));
      
      assert(result.success, 'Emotion screenshot should save');
      assert.strictEqual(result.data.directory, 'emotions');
      assert.strictEqual(result.data.actualFilename, `${emotionTest.emotion}_${emotionTest.intensity}.png`);
      
      const savedFile = path.join(testDir, 'emotions', `${emotionTest.emotion}_${emotionTest.intensity}.png`);
      assert(fs.existsSync(savedFile), 'Emotion animation screenshot should exist');
    });
  });

  describe('Performance and Reliability', () => {
    test('should handle multiple concurrent saves', async () => {
      const testDir = createTestDir();
      const dataURL = createTestDataURL();
      
      const savePromises = [];
      for (let i = 0; i < 5; i++) {
        savePromises.push(
          FileSaveCommand.execute(JSON.stringify({
            filename: `concurrent_${i}.png`,
            data: dataURL,
            baseDirectory: '.continuum/test-files'
          }))
        );
      }
      
      const results = await Promise.all(savePromises);
      
      results.forEach((result, index) => {
        assert(result.success, `Concurrent save ${index} should succeed`);
        const savedFile = path.join(testDir, `concurrent_${index}.png`);
        assert(fs.existsSync(savedFile), `Concurrent file ${index} should exist`);
      });
    });
  });
});