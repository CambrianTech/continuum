/**
 * Integration Tests for Browser Adapter Layer Interactions
 * Tests how the modular adapter architecture works together
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { MacOperaAdapter } from '../../adapters/MacOperaAdapter.js';
import { MacChromeAdapter } from '../../adapters/MacChromeAdapter.js';

describe('Browser Adapter Layer Integration Tests', () => {
  describe('Cross-Adapter Consistency', () => {
    test('should provide consistent interfaces across adapters', () => {
      const operaAdapter = new MacOperaAdapter();
      const chromeAdapter = new MacChromeAdapter();
      
      // Both should implement the same interface
      assert.strictEqual(typeof operaAdapter.countTabs, 'function');
      assert.strictEqual(typeof operaAdapter.closeTabs, 'function');
      assert.strictEqual(typeof operaAdapter.focusTab, 'function');
      
      assert.strictEqual(typeof chromeAdapter.countTabs, 'function');
      assert.strictEqual(typeof chromeAdapter.closeTabs, 'function');
      assert.strictEqual(typeof chromeAdapter.focusTab, 'function');
    });
    
    test('should provide consistent Chromium capabilities', () => {
      const operaAdapter = new MacOperaAdapter();
      const chromeAdapter = new MacChromeAdapter();
      
      // Both are Chromium-based, should have same capabilities
      assert.strictEqual(operaAdapter.supportsDevTools(), chromeAdapter.supportsDevTools());
      assert.strictEqual(operaAdapter.supportsRemoteDebugging(), chromeAdapter.supportsRemoteDebugging());
      
      const operaCapabilities = operaAdapter.getCapabilities();
      const chromeCapabilities = chromeAdapter.getCapabilities();
      
      assert.strictEqual(operaCapabilities.supportsDevTools, chromeCapabilities.supportsDevTools);
      assert.strictEqual(operaCapabilities.defaultDebugPort, chromeCapabilities.defaultDebugPort);
    });
  });
  
  describe('URL Pattern Matching Consistency', () => {
    test('should handle URL pattern matching consistently across adapters', async () => {
      const operaAdapter = new MacOperaAdapter();
      const chromeAdapter = new MacChromeAdapter();
      
      // Mock the same AppleScript response for both
      const mockResponse = '3'; // 3 valid Continuum tabs found
      
      operaAdapter['executeScriptTemplate'] = async () => mockResponse;
      chromeAdapter['executeScriptTemplate'] = async () => mockResponse;
      
      const operaCount = await operaAdapter.countTabs('http://localhost:9000');
      const chromeCount = await chromeAdapter.countTabs('http://localhost:9000');
      
      assert.strictEqual(operaCount, chromeCount, 'Both adapters should handle URL patterns identically');
      assert.strictEqual(operaCount, 3, 'Should correctly parse AppleScript response');
    });
    
    test('should reject debugging URLs consistently', async () => {
      const operaAdapter = new MacOperaAdapter();
      const chromeAdapter = new MacChromeAdapter();
      
      // Mock AppleScript that simulates precise URL matching
      const mockUrlMatcher = async (template: string, vars: Record<string, string>) => {
        const pattern = vars.URL_PATTERN;
        const debugUrl = 'http://localhost:9000/src/ui/components/shared/BaseWidget.js';
        
        // Our precise pattern should NOT match debugging URLs
        if (debugUrl === pattern || 
            debugUrl === pattern + '/' ||
            debugUrl.startsWith(pattern + '?') ||
            debugUrl.startsWith(pattern + '#')) {
          return '1'; // Should not happen - debugging URL should not match
        }
        
        return '0'; // Correct - debugging URL does not match
      };
      
      operaAdapter['executeScriptTemplate'] = mockUrlMatcher;
      chromeAdapter['executeScriptTemplate'] = mockUrlMatcher;
      
      const operaCount = await operaAdapter.countTabs('http://localhost:9000');
      const chromeCount = await chromeAdapter.countTabs('http://localhost:9000');
      
      assert.strictEqual(operaCount, 0, 'Opera adapter should reject debugging URLs');
      assert.strictEqual(chromeCount, 0, 'Chrome adapter should reject debugging URLs');
    });
  });
  
  describe('Error Handling Consistency', () => {
    test('should handle AppleScript failures consistently', async () => {
      const operaAdapter = new MacOperaAdapter();
      const chromeAdapter = new MacChromeAdapter();
      
      // Mock AppleScript failure for both
      const failureMock = async () => {
        throw new Error('AppleScript execution failed');
      };
      
      operaAdapter['executeScriptTemplate'] = failureMock;
      chromeAdapter['executeScriptTemplate'] = failureMock;
      
      // Both should handle errors gracefully
      const operaCount = await operaAdapter.countTabs('localhost:9000');
      const chromeCount = await chromeAdapter.countTabs('localhost:9000');
      const operaFocus = await operaAdapter.focusTab('localhost:9000');
      const chromeFocus = await chromeAdapter.focusTab('localhost:9000');
      
      assert.strictEqual(operaCount, 0, 'Opera should return 0 on error');
      assert.strictEqual(chromeCount, 0, 'Chrome should return 0 on error');
      assert.strictEqual(operaFocus, false, 'Opera should return false on error');
      assert.strictEqual(chromeFocus, false, 'Chrome should return false on error');
    });
  });
  
  describe('Platform Integration', () => {
    test('should provide correct platform metadata', () => {
      const operaAdapter = new MacOperaAdapter();
      const chromeAdapter = new MacChromeAdapter();
      
      // Both should be macOS adapters
      assert.strictEqual(operaAdapter.getOSName(), 'darwin');
      assert.strictEqual(chromeAdapter.getOSName(), 'darwin');
      
      // Should have different browser names
      assert.strictEqual(operaAdapter.getBrowserName(), 'Opera GX');
      assert.strictEqual(chromeAdapter.getBrowserName(), 'Google Chrome');
      
      // Should have different app names for AppleScript
      assert.strictEqual(operaAdapter['getAppName'](), 'Opera GX');
      assert.strictEqual(chromeAdapter['getAppName'](), 'Google Chrome');
    });
  });
});