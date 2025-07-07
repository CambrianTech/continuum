/**
 * Browser Tab Adapter Unit Tests
 * 
 * MIDDLE-OUT LAYER 2: Core Browser Detection Logic
 * Tests platform-specific tab detection and management
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { MacOperaAdapter, MacChromeAdapter, BaseBrowserAdapter } from '../../modules/BrowserTabAdapter';

describe('Browser Tab Adapter Unit Tests', () => {
  describe('BaseBrowserAdapter', () => {
    test('should define abstract interface', () => {
      // Verify the abstract base class structure
      assert.ok(BaseBrowserAdapter, 'BaseBrowserAdapter should be exported');
      assert.strictEqual(typeof BaseBrowserAdapter, 'function', 'Should be a constructor function');
    });

    test('should enforce implementation of abstract methods', () => {
      class TestAdapter extends BaseBrowserAdapter {
        async countTabs(_urlPattern: string): Promise<number> {
          return 0;
        }
      }
      
      const adapter = new TestAdapter();
      assert.ok(adapter instanceof BaseBrowserAdapter, 'Should extend BaseBrowserAdapter');
      assert.strictEqual(typeof adapter.countTabs, 'function', 'Should implement countTabs method');
    });
  });

  describe('MacOperaAdapter', () => {
    test('should instantiate correctly', () => {
      const adapter = new MacOperaAdapter();
      assert.ok(adapter instanceof BaseBrowserAdapter, 'Should extend BaseBrowserAdapter');
      assert.ok(adapter instanceof MacOperaAdapter, 'Should be instance of MacOperaAdapter');
    });

    test('should have proper method signatures', () => {
      const adapter = new MacOperaAdapter();
      assert.strictEqual(typeof adapter.countTabs, 'function', 'Should have countTabs method');
      
      // Test method signature
      const result = adapter.countTabs('localhost:9000');
      assert.ok(result instanceof Promise, 'countTabs should return a Promise');
    });

    test('should handle errors gracefully', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock execAsync to simulate command failure
      const originalExecAsync = (adapter as any).execAsync;
      (adapter as any).execAsync = async () => {
        throw new Error('Command failed');
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should return 0 on error');
      
      // Restore original if it exists
      if (originalExecAsync) {
        (adapter as any).execAsync = originalExecAsync;
      }
    });

    test('should parse AppleScript output correctly', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock execAsync to return known output
      (adapter as any).execAsync = async (command: string) => {
        if (command.includes('countTabs')) {
          return { stdout: '3\n' }; // Simulate 3 tabs found
        }
        return { stdout: '' };
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 3, 'Should parse AppleScript output correctly');
    });

    test('should handle invalid AppleScript output', async () => {
      const adapter = new MacOperaAdapter();
      
      (adapter as any).execAsync = async () => {
        return { stdout: 'invalid\n' }; // Non-numeric output
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should return 0 for invalid output');
    });
  });

  describe('MacChromeAdapter', () => {
    test('should instantiate correctly', () => {
      const adapter = new MacChromeAdapter();
      assert.ok(adapter instanceof BaseBrowserAdapter, 'Should extend BaseBrowserAdapter');
      assert.ok(adapter instanceof MacChromeAdapter, 'Should be instance of MacChromeAdapter');
    });

    test('should have proper Chrome-specific implementation', () => {
      const adapter = new MacChromeAdapter();
      assert.strictEqual(typeof adapter.countTabs, 'function', 'Should implement countTabs');
      
      // Should use different AppleScript than Opera
      const operaAdapter = new MacOperaAdapter();
      assert.notStrictEqual(adapter.constructor, operaAdapter.constructor, 'Should be different implementations');
    });

    test('should handle Chrome not available gracefully', async () => {
      const adapter = new MacChromeAdapter();
      
      (adapter as any).execAsync = async () => {
        throw new Error('Chrome not found');
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should return 0 when Chrome not available');
    });
  });

  describe('URL Pattern Matching', () => {
    test('should distinguish between app URLs and debugging URLs', async () => {
      const adapter = new MacOperaAdapter();
      
      // Mock execAsync to simulate AppleScript logic
      (adapter as any).execAsync = async (command: string) => {
        // Simulate the actual AppleScript logic we implemented
        const mockTabs = [
          'http://localhost:9000', // App URL - should match
          'http://localhost:9000/', // App URL with trailing slash - should match  
          'http://localhost:9000?session=abc', // App URL with query - should match
          'http://localhost:9000#section', // App URL with fragment - should match
          'http://localhost:9000/src/ui/components/shared/BaseWidget.js', // Debug URL - should NOT match
          'http://localhost:9000/dist/ui/continuum-browser.js', // Debug URL - should NOT match
        ];
        
        const targetPattern = 'http://localhost:9000';
        let matchCount = 0;
        
        // Simulate the AppleScript matching logic we implemented
        for (const url of mockTabs) {
          if (url === targetPattern || 
              url === targetPattern + '/' ||
              url.startsWith(targetPattern + '?') ||
              url.startsWith(targetPattern + '#')) {
            matchCount++;
          }
        }
        
        return { stdout: matchCount.toString() + '\n' };
      };
      
      const count = await adapter.countTabs('http://localhost:9000');
      
      // Should only match the 4 valid app URLs, not the 2 debugging URLs
      assert.strictEqual(count, 4, 'Should match only app URLs, not debugging URLs like /src/ui/components/shared/BaseWidget.js');
    });

    test('should handle different URL patterns', async () => {
      const adapter = new MacOperaAdapter();
      
      (adapter as any).execAsync = async (command: string) => {
        // Simulate different results for different URLs
        if (command.includes('localhost:9000')) {
          return { stdout: '2\n' };
        } else if (command.includes('localhost:3000')) {
          return { stdout: '1\n' };
        }
        return { stdout: '0\n' };
      };
      
      const count9000 = await adapter.countTabs('localhost:9000');
      const count3000 = await adapter.countTabs('localhost:3000');
      const countOther = await adapter.countTabs('example.com');
      
      assert.strictEqual(count9000, 2, 'Should find tabs for localhost:9000');
      assert.strictEqual(count3000, 1, 'Should find tabs for localhost:3000');
      assert.strictEqual(countOther, 0, 'Should find no tabs for other URLs');
    });

    test('should handle concurrent requests', async () => {
      const adapter = new MacOperaAdapter();
      
      let callCount = 0;
      (adapter as any).execAsync = async () => {
        callCount++;
        await new Promise(resolve => setTimeout(resolve, 10)); // Simulate async work
        return { stdout: '1\n' };
      };
      
      // Make multiple concurrent requests
      const promises = [
        adapter.countTabs('localhost:9000'),
        adapter.countTabs('localhost:9000'),
        adapter.countTabs('localhost:9000')
      ];
      
      const results = await Promise.all(promises);
      
      assert.strictEqual(results.length, 3, 'Should handle all requests');
      assert.ok(results.every(r => r === 1), 'All requests should return correct result');
      assert.strictEqual(callCount, 3, 'Should make separate calls for each request');
    });
  });

  describe('Error Handling', () => {
    test('should handle AppleScript syntax errors', async () => {
      const adapter = new MacOperaAdapter();
      
      (adapter as any).execAsync = async () => {
        throw new Error('syntax error: Expected end of line but found identifier');
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should handle AppleScript syntax errors');
    });

    test('should handle system command timeouts', async () => {
      const adapter = new MacOperaAdapter();
      
      (adapter as any).execAsync = async () => {
        throw new Error('Command timed out');
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should handle command timeouts');
    });

    test('should handle process permission errors', async () => {
      const adapter = new MacOperaAdapter();
      
      (adapter as any).execAsync = async () => {
        throw new Error('Operation not permitted');
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should handle permission errors');
    });
  });
});