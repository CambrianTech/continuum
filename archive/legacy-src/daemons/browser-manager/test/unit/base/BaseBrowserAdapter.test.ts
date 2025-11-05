/**
 * Unit Tests for BaseBrowserAdapter
 * Tests the abstract base interface and utility methods
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { BaseBrowserAdapter } from '../../../adapters/base/BaseBrowserAdapter.js';

// Test implementation for abstract base class
class TestBrowserAdapter extends BaseBrowserAdapter {
  constructor() {
    super('TestBrowser', 'TestOS');
  }
  
  async countTabs(_urlPattern: string): Promise<number> {
    return 1;
  }
  
  async closeTabs(_urlPattern: string): Promise<number> {
    return 0;
  }
  
  async focusTab(_urlPattern: string): Promise<boolean> {
    return true;
  }
  
  async isAvailable(): Promise<boolean> {
    return true;
  }
  
  async getBrowserVersion(): Promise<string | null> {
    return '1.0.0';
  }
}

describe('BaseBrowserAdapter Unit Tests', () => {
  describe('Abstract Interface', () => {
    test('should enforce implementation of required methods', () => {
      const adapter = new TestBrowserAdapter();
      
      assert.strictEqual(typeof adapter.countTabs, 'function');
      assert.strictEqual(typeof adapter.closeTabs, 'function');
      assert.strictEqual(typeof adapter.focusTab, 'function');
      assert.strictEqual(typeof adapter.isAvailable, 'function');
      assert.strictEqual(typeof adapter.getBrowserVersion, 'function');
    });
    
    test('should provide adapter metadata', () => {
      const adapter = new TestBrowserAdapter();
      
      assert.strictEqual(adapter.getBrowserName(), 'TestBrowser');
      assert.strictEqual(adapter.getOSName(), 'TestOS');
      
      const info = adapter.getAdapterInfo();
      assert.strictEqual(info.browser, 'TestBrowser');
      assert.strictEqual(info.os, 'TestOS');
      assert.strictEqual(info.type, 'TestBrowserAdapter');
    });
  });
  
  describe('Utility Methods', () => {
    test('should extract domain from URL correctly', () => {
      const adapter = new TestBrowserAdapter();
      
      assert.strictEqual(adapter['extractDomain']('http://localhost:9000'), 'localhost');
      assert.strictEqual(adapter['extractDomain']('https://example.com/path'), 'example.com');
      assert.strictEqual(adapter['extractDomain']('invalid-url'), 'invalid-url');
    });
    
    test('should detect relative imports correctly', () => {
      const adapter = new TestBrowserAdapter();
      
      assert.strictEqual(adapter['isRelativeImport']('./module'), true);
      assert.strictEqual(adapter['isRelativeImport']('../module'), true);
      assert.strictEqual(adapter['isRelativeImport']('absolute-module'), false);
      assert.strictEqual(adapter['isRelativeImport']('/absolute/path'), false);
    });
    
    test('should detect file extensions correctly', () => {
      const adapter = new TestBrowserAdapter();
      
      assert.strictEqual(adapter['hasFileExtension']('file.js'), true);
      assert.strictEqual(adapter['hasFileExtension']('file.ts'), true);
      assert.strictEqual(adapter['hasFileExtension']('file'), false);
      assert.strictEqual(adapter['hasFileExtension']('file.'), false);
    });
  });
});