/**
 * Simple Storage Module Tests
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';
import fs from 'node:fs';
import path from 'node:path';

const require = createRequire(import.meta.url);

describe('Persistent Storage - Basic Tests', () => {
  
  test('should be importable', () => {
    try {
      const PersistentStorage = require('../PersistentStorage.cjs');
      assert(typeof PersistentStorage === 'function', 'Should export a constructor function');
    } catch (error) {
      assert.fail(`Should be able to import PersistentStorage: ${error.message}`);
    }
  });

  test('should create instance with directory', () => {
    const PersistentStorage = require('../PersistentStorage.cjs');
    const testDir = path.join(process.cwd(), 'temp-test');
    
    try {
      const storage = new PersistentStorage(testDir);
      assert(storage, 'Should create storage instance');
      
      // Cleanup
      if (fs.existsSync(testDir)) {
        fs.rmSync(testDir, { recursive: true, force: true });
      }
    } catch (error) {
      assert.fail(`Should create storage instance: ${error.message}`);
    }
  });

  test('should have required methods', () => {
    const PersistentStorage = require('../PersistentStorage.cjs');
    const storage = new PersistentStorage('./temp');
    
    assert(typeof storage.save === 'function', 'Should have save method');
    assert(typeof storage.load === 'function', 'Should have load method');
  });
});