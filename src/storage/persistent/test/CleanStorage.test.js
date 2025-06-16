/**
 * Clean Storage Module Tests
 */
import { test, describe } from 'node:test';
import assert from 'node:assert';
import { createRequire } from 'module';
import fs from 'node:fs';
import path from 'node:path';

const require = createRequire(import.meta.url);
const PersistentStorage = require('../PersistentStorage.cjs');

// Test utilities
const TEST_DIR = path.join(process.cwd(), 'test-storage-clean');
const TEST_FILE = 'test-data.json';

function setupTest() {
  if (fs.existsSync(TEST_DIR)) {
    fs.rmSync(TEST_DIR, { recursive: true, force: true });
  }
  fs.mkdirSync(TEST_DIR, { recursive: true });
  return new PersistentStorage(TEST_DIR);
}

function cleanup() {
  if (fs.existsSync(TEST_DIR)) {
    fs.rmSync(TEST_DIR, { recursive: true, force: true });
  }
}

describe('Clean Storage Tests', () => {
  
  test('should create storage instance', () => {
    const storage = setupTest();
    try {
      assert(storage instanceof PersistentStorage, 'Should create PersistentStorage instance');
    } finally {
      cleanup();
    }
  });

  test('should save and load data with metadata', async () => {
    const storage = setupTest();
    try {
      const testData = { test: 'data', number: 42 };
      
      await storage.save(TEST_FILE, testData);
      const loaded = await storage.load(TEST_FILE);
      
      // Check original data is preserved
      assert.strictEqual(loaded.test, testData.test, 'Should preserve test field');
      assert.strictEqual(loaded.number, testData.number, 'Should preserve number field');
      
      // Check metadata was added
      assert(loaded._metadata, 'Should add metadata');
      assert(loaded._metadata.savedAt, 'Should add timestamp');
      assert.strictEqual(loaded._metadata.source, 'PersistentStorage', 'Should add source');
    } finally {
      cleanup();
    }
  });

  test('should handle missing files gracefully', async () => {
    const storage = setupTest();
    try {
      const result = await storage.load('nonexistent.json');
      // Check what the actual behavior is (could be null or empty object)
      assert(result !== undefined, 'Should return something for missing files');
    } finally {
      cleanup();
    }
  });

  test('should create nested directories', () => {
    const nestedDir = path.join(TEST_DIR, 'deep', 'nested', 'path');
    try {
      const storage = new PersistentStorage(nestedDir);
      assert(fs.existsSync(nestedDir), 'Should create nested directories');
      assert(storage instanceof PersistentStorage, 'Should create storage instance');
    } finally {
      cleanup();
    }
  });
});