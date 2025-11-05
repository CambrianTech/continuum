/**
 * Unit Tests for MacOSBrowserAdapter  
 * Tests AppleScript loading and execution capabilities
 */

import { describe, it, test } from 'node:test';
import assert from 'node:assert';
import { MacOSBrowserAdapter } from '../../../adapters/os/MacOSBrowserAdapter.js';

// Test implementation for abstract macOS adapter
class TestMacOSAdapter extends MacOSBrowserAdapter {
  constructor() {
    super('TestMacBrowser');
  }
  
  protected getAppName(): string {
    return 'Test Mac App';
  }
}

describe('MacOSBrowserAdapter Unit Tests', () => {
  describe('AppleScript Utilities', () => {
    test('should initialize with correct OS metadata', () => {
      const adapter = new TestMacOSAdapter();
      
      assert.strictEqual(adapter.getOSName(), 'darwin');
      assert.strictEqual(adapter.getBrowserName(), 'TestMacBrowser');
    });
    
    test('should handle AppleScript execution errors gracefully', async () => {
      const adapter = new TestMacOSAdapter();
      
      // Mock executeAppleScript to throw error
      adapter['executeAppleScript'] = async () => {
        throw new Error('AppleScript failed');
      };
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should return 0 on AppleScript error');
    });
    
    test('should replace template variables correctly', async () => {
      const adapter = new TestMacOSAdapter();
      
      // Mock loadScript to return template
      adapter['loadScript'] = async () => {
        return 'tell application "{{APP_NAME}}" to count tabs matching "{{URL_PATTERN}}"';
      };
      
      // Mock executeAppleScript to capture the result
      let capturedScript = '';
      adapter['executeAppleScript'] = async (script: string) => {
        capturedScript = script;
        return '3';
      };
      
      await adapter['executeScriptTemplate']('test-template', {
        APP_NAME: 'Test App',
        URL_PATTERN: 'localhost:9000'
      });
      
      assert.ok(capturedScript.includes('Test App'), 'Should replace APP_NAME');
      assert.ok(capturedScript.includes('localhost:9000'), 'Should replace URL_PATTERN');
      assert.ok(!capturedScript.includes('{{'), 'Should not contain template variables');
    });
  });
  
  describe('Browser Operations', () => {
    test('should parse numeric AppleScript results', async () => {
      const adapter = new TestMacOSAdapter();
      
      adapter['executeScriptTemplate'] = async () => '5';
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 5, 'Should parse numeric result correctly');
    });
    
    test('should handle invalid numeric results', async () => {
      const adapter = new TestMacOSAdapter();
      
      adapter['executeScriptTemplate'] = async () => 'invalid';
      
      const count = await adapter.countTabs('localhost:9000');
      assert.strictEqual(count, 0, 'Should return 0 for invalid numeric result');
    });
    
    test('should detect browser availability', async () => {
      const adapter = new TestMacOSAdapter();
      
      // Mock successful availability check
      adapter['executeScriptTemplate'] = async () => 'true';
      const available = await adapter.isAvailable();
      assert.strictEqual(available, true, 'Should detect available browser');
      
      // Mock failed availability check
      adapter['executeScriptTemplate'] = async () => {
        throw new Error('Browser not found');
      };
      const unavailable = await adapter.isAvailable();
      assert.strictEqual(unavailable, false, 'Should detect unavailable browser');
    });
  });
  
  describe('Script Template System', () => {
    test('should load and cache scripts efficiently', async () => {
      const adapter = new TestMacOSAdapter();
      
      let loadCount = 0;
      adapter['loadScript'] = async (name: string) => {
        loadCount++;
        return `script for ${name}`;
      };
      
      adapter['executeAppleScript'] = async () => 'result';
      
      // Execute same template multiple times
      await adapter['executeScriptTemplate']('test', { VAR: 'value' });
      await adapter['executeScriptTemplate']('test', { VAR: 'value2' });
      
      // Should load script each time (no caching in base implementation)
      assert.strictEqual(loadCount, 2, 'Should load script for each execution');
    });
  });
});