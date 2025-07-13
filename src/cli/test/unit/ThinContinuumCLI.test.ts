#!/usr/bin/env tsx
/**
 * Unit Tests: ThinContinuumCLI - Universal Command Interface
 * 
 * Tests the ultra-thin CLI that forwards commands exactly as named.
 * No translations, no god objects - pure delegation to command modules.
 */

import { strict as assert } from 'node:assert';
import { test, describe } from 'node:test';

// Import the CLI class for testing
import { ThinContinuumCLI } from '../../continuum-cli.js';

describe('ThinContinuumCLI - Universal Command Interface', () => {
  let cli: ThinContinuumCLI;

  test('setup', () => {
    cli = new ThinContinuumCLI();
    assert.ok(cli, 'CLI instance should be created');
  });

  describe('parseArgs - Universal Argument Parsing', () => {
    test('should parse first argument as command (empty args handled by run method)', () => {
      // Note: parseArgs is not called with empty args in real usage - run() handles that case
      const result = cli.parseArgs(['health', '--detailed']);
      assert.strictEqual(result.command, 'health');
      assert.deepStrictEqual(result.rawArgs, ['--detailed']);
    });

    test('should parse regular commands correctly', () => {
      const result = cli.parseArgs(['screenshot', '--selector=body', '--scale=2']);
      assert.strictEqual(result.command, 'screenshot');
      assert.deepStrictEqual(result.rawArgs, ['--selector=body', '--scale=2']);
    });

    test('should handle universal --command syntax', () => {
      const result = cli.parseArgs(['--health', '--detailed']);
      assert.strictEqual(result.command, 'health');
      assert.deepStrictEqual(result.rawArgs, ['--detailed']);
    });

    test('should handle complex parameter sets', () => {
      const result = cli.parseArgs(['data-marshal', '--operation=encode', '--data={"test":"value"}', '--encoding=json']);
      assert.strictEqual(result.command, 'data-marshal');
      assert.deepStrictEqual(result.rawArgs, ['--operation=encode', '--data={"test":"value"}', '--encoding=json']);
    });

    test('should handle commands with no parameters', () => {
      const result = cli.parseArgs(['agents']);
      assert.strictEqual(result.command, 'agents');
      assert.deepStrictEqual(result.rawArgs, []);
    });

    test('should handle universal syntax with no parameters', () => {
      const result = cli.parseArgs(['--projects']);
      assert.strictEqual(result.command, 'projects');
      assert.deepStrictEqual(result.rawArgs, []);
    });
  });

  describe('adaptCommand - No Translation Policy', () => {
    test('should never translate commands - pure pass-through', async () => {
      const result = await cli.adaptCommand('screenshot', ['--filename=test.png']);
      assert.strictEqual(result.command, 'screenshot');
      assert.deepStrictEqual(result.rawArgs, ['--filename=test.png']);
    });

    test('should not translate pic to screenshot', async () => {
      const result = await cli.adaptCommand('pic', ['--scale=2']);
      assert.strictEqual(result.command, 'pic'); // Should stay as pic (will fail)
      assert.deepStrictEqual(result.rawArgs, ['--scale=2']);
    });

    test('should not translate snap to screenshot', async () => {
      const result = await cli.adaptCommand('snap', []);
      assert.strictEqual(result.command, 'snap'); // Should stay as snap (will fail)
      assert.deepStrictEqual(result.rawArgs, []);
    });

    test('should pass through unknown commands unchanged', async () => {
      const result = await cli.adaptCommand('nonexistent', ['--some=param']);
      assert.strictEqual(result.command, 'nonexistent');
      assert.deepStrictEqual(result.rawArgs, ['--some=param']);
    });

    test('should handle complex nested parameters', async () => {
      const complexData = '--data={"complex":{"nested":{"data":"value"}}}';
      const result = await cli.adaptCommand('data-marshal', ['--operation=encode', complexData, '--encoding=json']);
      assert.strictEqual(result.command, 'data-marshal');
      assert.deepStrictEqual(result.rawArgs, ['--operation=encode', complexData, '--encoding=json']);
    });
  });

  describe('buildRequestPayload - Parameter Forwarding', () => {
    test('should create proper args array format', () => {
      const payload = cli.buildRequestPayload(['--method=widgets', '--selector=body']);
      assert.deepStrictEqual(payload, {
        args: ['--method=widgets', '--selector=body']
      });
    });

    test('should handle empty arguments', () => {
      const payload = cli.buildRequestPayload([]);
      assert.deepStrictEqual(payload, {
        args: []
      });
    });

    test('should preserve complex JSON in arguments', () => {
      const jsonArg = '--data={"complex":{"nested":true}}';
      const payload = cli.buildRequestPayload([jsonArg]);
      assert.deepStrictEqual(payload, {
        args: [jsonArg]
      });
    });
  });

  describe('Ultra-Thin Architecture Validation', () => {
    test('should not contain command-specific logic', () => {
      // CLI should have no hardcoded knowledge of specific commands
      const instance = new ThinContinuumCLI();
      
      // Should not have command registries or static command lists
      assert.strictEqual((instance as any).commands, undefined);
      assert.strictEqual((instance as any).commandList, undefined);
      assert.strictEqual((instance as any).availableCommands, undefined);
    });

    test('should delegate all command knowledge to system', () => {
      // Verify that CLI uses pure delegation, not static knowledge
      const instance = new ThinContinuumCLI();
      
      // Should not have any command-specific methods
      assert.strictEqual(typeof (instance as any).handleScreenshot, 'undefined');
      assert.strictEqual(typeof (instance as any).handleHealth, 'undefined');
      assert.strictEqual(typeof (instance as any).handleAgents, 'undefined');
    });

    test('should maintain universal syntax support', () => {
      // Both regular and --flag syntax should work identically
      const regular = cli.parseArgs(['health', '--detailed']);
      const universal = cli.parseArgs(['--health', '--detailed']);
      
      assert.strictEqual(regular.command, universal.command);
      assert.deepStrictEqual(regular.rawArgs, universal.rawArgs);
    });
  });
});

// Entry point for direct execution
if (import.meta.url === `file://${process.argv[1]}`) {
  console.log('🧪 Running CLI unit tests...');
}