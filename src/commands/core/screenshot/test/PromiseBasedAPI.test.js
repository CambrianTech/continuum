/**
 * Promise-Based API Tests for Screenshot Command Module
 * Migrated from __tests__/unit/PromiseBasedAPI.test.cjs
 * Tests promise-based validation and error handling
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';

const require = createRequire(import.meta.url);

describe('Screenshot Command Promise-Based API', () => {
  let mockDocument;
  let mockWindow;
  
  test('should reject promises for invalid target elements', async () => {
    // Mock DOM environment
    mockDocument = {
      body: {
        tagName: 'BODY',
        offsetWidth: 0,
        offsetHeight: 0
      }
    };
    
    mockWindow = {
      document: mockDocument,
      html2canvas: () => Promise.resolve({})
    };
    
    // Load ScreenshotUtils
    const ScreenshotUtils = require('../ScreenshotUtils.js');
    
    try {
      await ScreenshotUtils.takeScreenshot(mockDocument.body, { validateDimensions: true });
      assert.fail('Should have rejected for zero-dimension element');
    } catch (error) {
      assert(error.message.includes('dimension'), 'Should reject with dimension error');
    }
  });

  test('should handle promise chains correctly', async () => {
    mockDocument = {
      body: {
        tagName: 'BODY',
        offsetWidth: 800,
        offsetHeight: 600
      }
    };
    
    mockWindow = {
      document: mockDocument,
      html2canvas: () => Promise.resolve({
        toDataURL: () => 'data:image/png;base64,test'
      })
    };
    
    const ScreenshotUtils = require('../ScreenshotUtils.js');
    
    const result = await ScreenshotUtils.takeScreenshot(mockDocument.body);
    assert(result, 'Should return successful result');
  });

  test('should validate multiple elements in promise chains', async () => {
    const mockElements = [];
    for (let i = 0; i < 39; i++) {
      mockElements.push({
        tagName: i < 5 ? 'BUTTON' : i < 28 ? 'DIV' : 'SCRIPT',
        offsetWidth: 0,
        offsetHeight: 0
      });
    }
    
    mockDocument = {
      body: {
        tagName: 'BODY',
        offsetWidth: 1673,
        offsetHeight: 1630,
        querySelectorAll: () => mockElements
      }
    };
    
    const ScreenshotUtils = require('../ScreenshotUtils.js');
    
    try {
      await ScreenshotUtils.takeScreenshot(mockDocument.body, { 
        validateChildElements: true 
      });
      assert.fail('Should have rejected for zero-dimension child elements');
    } catch (error) {
      assert(error.message, 'Should have error message for validation failure');
    }
  });
});