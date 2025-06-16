/**
 * Persistent Storage Module Tests
 * Tests the modular persistent storage functionality
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';
import fs from 'node:fs';
import path from 'node:path';

const require = createRequire(import.meta.url);
const PersistentStorage = require('../PersistentStorage.cjs');

// Test utilities
const TEST_DIR = path.join(process.cwd(), 'test-storage');
const TEST_FILE = 'test-data.json';

function setupTest() {
  // Clean test directory
  if (fs.existsSync(TEST_DIR)) {
    fs.rmSync(TEST_DIR, { recursive: true, force: true });
  }
  fs.mkdirSync(TEST_DIR, { recursive: true });
  
  return new PersistentStorage(TEST_DIR);
}

function cleanup() {
  // Cleanup
  if (fs.existsSync(TEST_DIR)) {
    fs.rmSync(TEST_DIR, { recursive: true, force: true });
  }
}

describe('Persistent Storage Module', () => {
  
  test('should create storage instance', () => {
    const storage = setupTest();
    try {
      assert(storage instanceof PersistentStorage, 'Should create PersistentStorage instance');
      assert.strictEqual(storage.baseDir, TEST_DIR, 'Should set correct base directory');
    } finally {
      cleanup();
    }
  });

  test('should save and load data', async () => {
    const storage = setupTest();
    try {
      const testData = { test: 'data', number: 42 };
      
      await storage.save(TEST_FILE, testData);
      const loaded = await storage.load(TEST_FILE);
      
      // Storage adds metadata, so check the original data is preserved
      assert.strictEqual(loaded.test, testData.test, 'Should preserve test field');
      assert.strictEqual(loaded.number, testData.number, 'Should preserve number field');
      assert(loaded._metadata, 'Should add metadata');
      assert(loaded._metadata.savedAt, 'Should add timestamp');
    } finally {
      cleanup();
    }
  });

  test('should handle missing files gracefully', async () => {
    const storage = setupTest();
    try {
      const result = await storage.load('nonexistent.json');
      assert.strictEqual(result, null, 'Should return null for missing files');
    } finally {
      cleanup();
    }
  });

  test('should validate storage directory creation', () => {
    assert(storage instanceof PersistentStorage, 'Should create PersistentStorage instance');
    assert.strictEqual(storage.baseDir, TEST_DIR, 'Should set correct base directory');
  });

  test('should save and load data', async () => {
    const testData = { test: 'data', number: 42 };
    
    await storage.save(TEST_FILE, testData);
    const loaded = await storage.load(TEST_FILE);
    
    assert.deepStrictEqual(loaded, testData, 'Should save and load data correctly');
  });

  test('should handle missing files gracefully', async () => {
    const result = await storage.load('nonexistent.json');
    assert.strictEqual(result, null, 'Should return null for missing files');
  });

  test('should validate storage directory creation', () => {
    const newDir = path.join(TEST_DIR, 'nested', 'directory');
    const newStorage = new PersistentStorage(newDir);
    
    assert(fs.existsSync(newDir), 'Should create nested directories');
  });

  test('should handle filesystem errors gracefully', async () => {
    // Test with invalid directory permissions (if possible)
    try {
      await storage.save('../../../invalid/path.json', { data: 'test' });
      assert.fail('Should have thrown an error for invalid path');
    } catch (error) {
      assert(error instanceof Error, 'Should throw error for invalid operations');
    }
  });
});