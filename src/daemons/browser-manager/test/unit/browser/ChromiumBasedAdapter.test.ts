/**
 * Unit Tests for ChromiumBasedAdapter
 * Tests Chromium-specific browser logic and capabilities
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { ChromiumBasedAdapter } from '../../../adapters/browser/ChromiumBasedAdapter.js';

describe('ChromiumBasedAdapter Unit Tests', () => {
  describe('URL Pattern Matching', () => {
    test('should correctly identify valid Continuum URLs', () => {
      const adapter = new ChromiumBasedAdapter();
      const pattern = 'http://localhost:9000';
      
      // Valid URLs that should match
      assert.strictEqual(adapter['isValidContinuumURL'](pattern, pattern), true, 'Exact match');
      assert.strictEqual(adapter['isValidContinuumURL'](pattern + '/', pattern), true, 'With trailing slash');
      assert.strictEqual(adapter['isValidContinuumURL'](pattern + '?session=abc', pattern), true, 'With query params');
      assert.strictEqual(adapter['isValidContinuumURL'](pattern + '#section', pattern), true, 'With fragment');
    });
    
    test('should reject debugging URLs', () => {
      const adapter = new ChromiumBasedAdapter();
      const pattern = 'http://localhost:9000';
      
      // Invalid URLs that should NOT match
      assert.strictEqual(adapter['isValidContinuumURL'](pattern + '/src/ui/components/BaseWidget.js', pattern), false, 'Debugging URL');
      assert.strictEqual(adapter['isValidContinuumURL'](pattern + '/dist/ui/continuum-browser.js', pattern), false, 'Asset URL');
      assert.strictEqual(adapter['isValidContinuumURL'](pattern + '/api/commands/health', pattern), false, 'API URL');
      assert.strictEqual(adapter['isValidContinuumURL']('http://localhost:9001', pattern), false, 'Different port');
    });
  });
  
  describe('DevTools Protocol Support', () => {
    test('should provide correct DevTools configuration', () => {
      const adapter = new ChromiumBasedAdapter();
      
      assert.strictEqual(adapter.supportsDevTools(), true, 'Should support DevTools');
      assert.strictEqual(adapter.supportsRemoteDebugging(), true, 'Should support remote debugging');
      assert.strictEqual(adapter['getDebugPort'](), 9222, 'Should use default debug port');
      assert.strictEqual(adapter['getDevToolsURL'](9222), 'http://localhost:9222', 'Should generate correct DevTools URL');
    });
    
    test('should provide Chromium-specific capabilities', () => {
      const adapter = new ChromiumBasedAdapter();
      const capabilities = adapter.getCapabilities();
      
      assert.strictEqual(capabilities.supportsDevTools, true);
      assert.strictEqual(capabilities.supportsRemoteDebugging, true);
      assert.strictEqual(capabilities.defaultDebugPort, 9222);
      assert.ok(Array.isArray(capabilities.supportedExtensions));
      assert.ok(capabilities.supportedExtensions.includes('.crx'));
    });
  });
  
  describe('Command Line Arguments', () => {
    test('should provide common Chromium arguments', () => {
      const adapter = new ChromiumBasedAdapter();
      const args = adapter['getCommonChromiumArgs']();
      
      assert.ok(Array.isArray(args), 'Should return array of arguments');
      assert.ok(args.includes('--no-first-run'), 'Should disable first run');
      assert.ok(args.includes('--no-default-browser-check'), 'Should disable default browser check');
      assert.ok(args.some(arg => arg.includes('--disable-')), 'Should include disable flags');
    });
    
    test('should provide debug-specific arguments', () => {
      const adapter = new ChromiumBasedAdapter();
      const debugArgs = adapter['getDebugArgs'](9999);
      
      assert.ok(debugArgs.includes('--remote-debugging-port=9999'), 'Should set debug port');
      assert.ok(debugArgs.includes('--remote-debugging-address=127.0.0.1'), 'Should set debug address');
    });
  });
  
  describe('Platform-Specific Paths', () => {
    test('should provide correct user data directory for platform', () => {
      const adapter = new ChromiumBasedAdapter();
      const userDataDir = adapter['getChromiumUserDataDir']();
      
      // Should return platform-appropriate path
      assert.strictEqual(typeof userDataDir, 'string');
      assert.ok(userDataDir.length > 0);
      
      // On macOS should contain Application Support
      if (process.platform === 'darwin') {
        assert.ok(userDataDir.includes('Application Support'));
      }
    });
  });
});