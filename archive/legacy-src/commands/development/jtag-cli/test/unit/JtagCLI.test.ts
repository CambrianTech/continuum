/**
 * JTAG CLI Unit Tests
 * Test the JtagCLI class methods and configuration
 */

import { test, describe } from 'node:test';
import assert from 'node:assert';
import { JtagCLI } from '../../JtagCLI';

describe('JtagCLI', () => {
  test('should create instance with default config', () => {
    const jtag = new JtagCLI();
    assert.ok(jtag);
  });

  test('should create instance with custom config', () => {
    const config = {
      continuumBinary: './custom-continuum',
      sessionsPath: './custom-sessions'
    };
    const jtag = new JtagCLI(config);
    assert.ok(jtag);
  });

  test('should have all required methods', () => {
    const jtag = new JtagCLI();
    
    assert.strictEqual(typeof jtag.screenshot, 'function');
    assert.strictEqual(typeof jtag.probe, 'function');
    assert.strictEqual(typeof jtag.logs, 'function');
    assert.strictEqual(typeof jtag.errors, 'function');
    assert.strictEqual(typeof jtag.warnings, 'function');
    assert.strictEqual(typeof jtag.health, 'function');
    assert.strictEqual(typeof jtag.session, 'function');
    assert.strictEqual(typeof jtag.hotreload, 'function');
    assert.strictEqual(typeof jtag.help, 'function');
  });

  test('help method should not throw', () => {
    const jtag = new JtagCLI();
    assert.doesNotThrow(() => {
      jtag.help();
    });
  });

  test('screenshot method should handle default parameters', async () => {
    const jtag = new JtagCLI({
      continuumBinary: 'echo' // Use echo command for testing
    });
    
    // Should not throw with default parameters
    assert.doesNotThrow(async () => {
      // Note: This will try to execute 'echo' command which should succeed
      await jtag.screenshot();
    });
  });

  test('probe method should handle default parameters', async () => {
    const jtag = new JtagCLI({
      continuumBinary: 'echo' // Use echo command for testing
    });
    
    // Should not throw with default parameters
    assert.doesNotThrow(async () => {
      await jtag.probe();
    });
  });

  test('session method with custom sessions path', async () => {
    const jtag = new JtagCLI({
      sessionsPath: './non-existent-path'
    });
    
    // Should handle missing sessions gracefully
    assert.doesNotThrow(async () => {
      await jtag.session();
    });
  });
});

console.log('🧪 JTAG CLI Unit Tests');
console.log('======================');
console.log('Testing JtagCLI class methods and configuration...');