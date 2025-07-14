// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * Layer 3: Command System Tests - FileReadCommand Parameter Parsing
 * 
 * 🔬 MIDDLE-OUT TESTING: Layer 3 - Command execution with parameter parsing
 * 
 * Tests the exact REST API failure scenario:
 * curl -X POST /api/commands/read -d '{"args":["--file", "package.json"]}'
 * Returns: {"content": null} instead of file content
 * 
 * 🚨 CROSS-CUTTING CONCERN: CLI parameter mapping --file vs --filename
 */

import { describe, it } from 'node:test';
import assert from 'node:assert';
import { FileReadCommand } from '../../FileReadCommand';

describe('FileReadCommand - REST API Parameter Parsing Issue', () => {
  
  it('should handle direct filename parameter (baseline)', async () => {
    const params = { filename: 'package.json' };
    const result = await FileReadCommand.execute(params);
    
    console.log('✅ Direct filename result:', result.success ? 'SUCCESS' : `FAILED: ${result.error}`);
    assert.strictEqual(result.success, true, `Direct filename failed: ${result.error}`);
  });
  
  it('should handle CLI args --filename format', async () => {
    const params = { args: ['--filename', 'package.json'] };
    const result = await FileReadCommand.execute(params);
    
    console.log('🔍 CLI --filename result:', result.success ? 'SUCCESS' : `FAILED: ${result.error}`);
    assert.strictEqual(result.success, true, `CLI --filename failed: ${result.error}`);
  });
  
  it('should handle CLI args --file format (REST API issue)', async () => {
    // This is the EXACT format causing the REST API to return null content
    const params = { args: ['--file', 'package.json'] };
    const result = await FileReadCommand.execute(params);
    
    console.log('🚨 CLI --file result:', result.success ? 'SUCCESS' : `FAILED: ${result.error}`);
    console.log('🔍 Result data:', JSON.stringify(result.data, null, 2));
    
    // This assertion will likely FAIL, showing us the parameter parsing issue
    assert.strictEqual(result.success, true, `CLI --file failed: ${result.error}`);
    
    if (result.success) {
      assert.ok(result.data?.content, 'Content should not be null');
      assert.strictEqual(typeof result.data.content, 'string', 'Content should be a string');
    }
  });
  
});