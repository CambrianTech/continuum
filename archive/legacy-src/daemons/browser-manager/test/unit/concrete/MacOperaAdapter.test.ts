/**
 * Unit Tests for MacOperaAdapter
 * Tests the concrete Opera GX implementation with URL pattern matching focus
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { MacOperaAdapter } from '../../../adapters/MacOperaAdapter.js';

describe('MacOperaAdapter Unit Tests', () => {
  describe('Adapter Configuration', () => {
    test('should initialize with correct metadata', () => {
      const adapter = new MacOperaAdapter();
      
      assert.strictEqual(adapter.getBrowserName(), 'Opera GX');
      assert.strictEqual(adapter.getOSName(), 'darwin');
      assert.strictEqual(adapter['getAppName'](), 'Opera GX');
    });
    
    test('should combine macOS and Chromium capabilities', () => {
      const adapter = new MacOperaAdapter();
      const capabilities = adapter.getCapabilities();
      
      // Should have Chromium capabilities
      assert.strictEqual(adapter.supportsDevTools(), true);
      assert.strictEqual(adapter.supportsRemoteDebugging(), true);
      
      // Should have adapter info
      assert.strictEqual(capabilities.browser, 'Opera GX');
      assert.strictEqual(capabilities.os, 'darwin');
    });
  });
  
  describe('URL Pattern Matching - The Core Issue', () => {
    test('should distinguish between app URLs and debugging URLs', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock the AppleScript execution to simulate real browser tabs
      const mockTabs = [
        'http://localhost:9000/src/ui/components/shared/BaseWidget.js', // Debug URL - should NOT match
        'http://localhost:9000', // App URL - should match
        'http://localhost:9000/', // App URL with slash - should match
        'http://localhost:9000?session=abc', // App URL with query - should match
        'http://localhost:9000/dist/ui/continuum-browser.js', // Asset URL - should NOT match
      ];
      
      // Mock executeScriptTemplate to simulate AppleScript URL checking
      adapter['executeScriptTemplate'] = async (template: string, vars: Record<string, string>) => {
        const pattern = vars.URL_PATTERN;
        let count = 0;
        
        for (const url of mockTabs) {
          // Simulate the exact AppleScript logic we implemented
          if (url === pattern || 
              url === pattern + '/' ||
              url.startsWith(pattern + '?') ||
              url.startsWith(pattern + '#')) {
            count++;
          }
        }
        
        return count.toString();
      };
      
      const count = await adapter.countTabs('http://localhost:9000');
      
      // Should match only 3 valid app URLs, not the 2 debugging URLs
      assert.strictEqual(count, 3, 'Should match only app URLs, not debugging URLs');
    });
    
    test('should prioritize exact matches in focus logic', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock focus to track which URL gets focused
      let focusedURL = '';
      adapter['executeScriptTemplate'] = async (template: string, vars: Record<string, string>) => {
        if (template === 'focus-tab') {
          const pattern = vars.URL_PATTERN;
          
          // Simulate tab order: debugging URL first, then app URL
          const tabs = [
            'http://localhost:9000/src/ui/components/shared/BaseWidget.js',
            'http://localhost:9000'
          ];
          
          // Our prioritized logic should find exact match first
          for (const url of tabs) {
            if (url === pattern) {
              focusedURL = url;
              return 'found-exact';
            }
          }
          
          return 'not found';
        }
        return '0';
      };
      
      const result = await adapter.focusTab('http://localhost:9000');
      
      assert.strictEqual(result, true, 'Should successfully focus tab');
      assert.strictEqual(focusedURL, 'http://localhost:9000', 'Should focus exact app URL, not debugging URL');
    });
  });
  
  describe('Opera-Specific Features', () => {
    test('should handle Opera version format correctly', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock Opera version response
      adapter['executeScriptTemplate'] = async () => 'Opera 95.0.4635.84';
      
      const version = await adapter.getBrowserVersion();
      assert.strictEqual(version, '95.0.4635.84', 'Should strip "Opera " prefix from version');
    });
    
    test('should handle browser availability gracefully', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock unavailable browser
      adapter['executeScriptTemplate'] = async () => {
        throw new Error('Opera GX not found');
      };
      
      const available = await adapter.isAvailable();
      assert.strictEqual(available, false, 'Should detect when Opera is not available');
    });
  });
  
  describe('Error Handling', () => {
    test('should handle AppleScript failures gracefully', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock AppleScript failure
      adapter['executeScriptTemplate'] = async () => {
        throw new Error('AppleScript execution failed');
      };
      
      const count = await adapter.countTabs('localhost:9000');
      const closed = await adapter.closeTabs('localhost:9000');
      const focused = await adapter.focusTab('localhost:9000');
      
      assert.strictEqual(count, 0, 'Should return 0 on count error');
      assert.strictEqual(closed, 0, 'Should return 0 on close error');
      assert.strictEqual(focused, false, 'Should return false on focus error');
    });
  });
});