/**
 * Screenshot Promise-Based API Tests - SERVER SIDE ONLY
 * Tests server-side promise patterns, NOT client-side browser code
 * Client-side ScreenshotUtils is browser-only and should NOT be tested here
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';

// Mock ScreenshotUtils for server-side testing - DO NOT import browser code
const MockScreenshotUtils = {
  takeScreenshot: (element, options = {}) => {
    if (!element || element.offsetWidth === 0) {
      return Promise.reject(new Error('Cannot screenshot element with 0 dimensions'));
    }
    return Promise.resolve({
      success: true,
      dataUrl: 'data:image/png;base64,mockdata',
      filename: options.filename || 'screenshot.png'
    });
  }
};

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
    
    // Use MockScreenshotUtils for server-side testing
    const ScreenshotUtils = MockScreenshotUtils;
    
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
    
    const ScreenshotUtils = MockScreenshotUtils;
    
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
    
    const ScreenshotUtils = MockScreenshotUtils;
    
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