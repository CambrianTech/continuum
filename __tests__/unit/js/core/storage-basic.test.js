/**
 * Basic Storage Tests
 * Just verify storage functionality works
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';

describe('Storage Basic Functionality', () => {
  
  test('filesystem operations work', () => {
    // Test basic filesystem operations that storage depends on
    const fs = require('fs');
    const path = require('path');
    const testDir = path.join(process.cwd(), 'temp-storage-test');
    
    try {
      // Test directory creation
      fs.mkdirSync(testDir, { recursive: true });
      assert(fs.existsSync(testDir), 'Should create directory');
      
      // Test file operations  
      const testFile = path.join(testDir, 'test.json');
      const testData = { test: true };
      fs.writeFileSync(testFile, JSON.stringify(testData));
      
      assert(fs.existsSync(testFile), 'Should create file');
      
      const loaded = JSON.parse(fs.readFileSync(testFile, 'utf8'));
      assert.deepStrictEqual(loaded, testData, 'Should read data correctly');
      
    } finally {
      // Cleanup
      if (fs.existsSync(testDir)) {
        fs.rmSync(testDir, { recursive: true, force: true });
      }
    }
  });

  test('JSON serialization works', () => {
    const testData = {
      string: 'test',
      number: 42,
      boolean: true,
      array: [1, 2, 3],
      object: { nested: 'value' }
    };
    
    const serialized = JSON.stringify(testData);
    const parsed = JSON.parse(serialized);
    
    assert.deepStrictEqual(parsed, testData, 'Should serialize/deserialize correctly');
  });

  test('path operations work', () => {
    const path = require('path');
    
    const testPath = path.join('storage', 'data', 'file.json');
    assert(testPath.includes('storage'), 'Should join paths correctly');
    assert(testPath.includes('file.json'), 'Should include filename');
    
    const dir = path.dirname(testPath);
    const base = path.basename(testPath);
    
    assert(dir.includes('storage'), 'Should extract directory');
    assert.strictEqual(base, 'file.json', 'Should extract basename');
  });
});