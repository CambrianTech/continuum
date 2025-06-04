/**
 * Unit Tests for PersistentStorage
 * Tests the abstracted storage mechanism for saving/loading JSON data
 */

const fs = require('fs');
const path = require('path');
const PersistentStorage = require('../../src/storage/PersistentStorage.cjs');

// Test utilities
const TEST_DIR = path.join(__dirname, 'test-storage');
const TEST_FILE = 'test-data.json';

describe('PersistentStorage', () => {
  let storage;
  
  beforeEach(() => {
    // Create fresh storage instance for each test
    storage = new PersistentStorage(TEST_DIR);
    
    // Clean up test directory before each test
    if (fs.existsSync(TEST_DIR)) {
      fs.rmSync(TEST_DIR, { recursive: true, force: true });
    }
  });
  
  afterEach(() => {
    // Clean up test directory after each test
    if (fs.existsSync(TEST_DIR)) {
      fs.rmSync(TEST_DIR, { recursive: true, force: true });
    }
  });

  describe('Constructor and Directory Management', () => {
    test('should create storage directory on initialization', () => {
      expect(fs.existsSync(TEST_DIR)).toBe(true);
    });

    test('should handle existing directory gracefully', () => {
      // Create directory manually
      fs.mkdirSync(TEST_DIR, { recursive: true });
      
      // Should not throw when creating storage with existing directory
      expect(() => {
        new PersistentStorage(TEST_DIR);
      }).not.toThrow();
    });

    test('should use default .continuum directory when no baseDir provided', () => {
      const defaultStorage = new PersistentStorage();
      expect(defaultStorage.baseDir).toBe('.continuum');
    });
  });

  describe('File Path Management', () => {
    test('should generate correct file paths', () => {
      const filePath = storage.getFilePath('test.json');
      expect(filePath).toBe(path.join(TEST_DIR, 'test.json'));
    });

    test('should handle nested file paths', () => {
      const filePath = storage.getFilePath('nested/test.json');
      expect(filePath).toBe(path.join(TEST_DIR, 'nested/test.json'));
    });
  });

  describe('Save Operations', () => {
    test('should save data successfully', () => {
      const testData = { message: 'Hello World', count: 42 };
      
      const result = storage.save(TEST_FILE, testData);
      
      expect(result).toBe(true);
      expect(fs.existsSync(storage.getFilePath(TEST_FILE))).toBe(true);
    });

    test('should add metadata to saved data', () => {
      const testData = { message: 'Hello World' };
      
      storage.save(TEST_FILE, testData, { source: 'UnitTest', version: '2.0.0' });
      
      const savedContent = JSON.parse(fs.readFileSync(storage.getFilePath(TEST_FILE), 'utf8'));
      expect(savedContent._metadata).toBeDefined();
      expect(savedContent._metadata.source).toBe('UnitTest');
      expect(savedContent._metadata.version).toBe('2.0.0');
      expect(savedContent._metadata.savedAt).toBeDefined();
      expect(savedContent.message).toBe('Hello World');
    });

    test('should use default metadata when options not provided', () => {
      const testData = { message: 'Hello World' };
      
      storage.save(TEST_FILE, testData);
      
      const savedContent = JSON.parse(fs.readFileSync(storage.getFilePath(TEST_FILE), 'utf8'));
      expect(savedContent._metadata.source).toBe('PersistentStorage');
      expect(savedContent._metadata.version).toBe('1.0.0');
    });

    test('should handle save errors gracefully', () => {
      // Try to save to an invalid location
      const invalidStorage = new PersistentStorage('/root/invalid-path');
      
      const result = invalidStorage.save(TEST_FILE, { test: 'data' });
      
      expect(result).toBe(false);
    });

    test('should format JSON with proper indentation', () => {
      const testData = { nested: { value: 'test' } };
      
      storage.save(TEST_FILE, testData);
      
      const savedContent = fs.readFileSync(storage.getFilePath(TEST_FILE), 'utf8');
      expect(savedContent).toContain('  '); // Should have indentation
      expect(savedContent).toContain('\\n'); // Should have newlines
    });
  });

  describe('Load Operations', () => {
    test('should load data successfully', () => {
      const testData = { message: 'Hello World', count: 42 };
      
      // Save data first
      storage.save(TEST_FILE, testData);
      
      // Load and verify
      const loadedData = storage.load(TEST_FILE);
      
      expect(loadedData).toBeDefined();
      expect(loadedData.message).toBe('Hello World');
      expect(loadedData.count).toBe(42);
      expect(loadedData._metadata).toBeDefined();
    });

    test('should return null for non-existent files', () => {
      const loadedData = storage.load('non-existent.json');
      
      expect(loadedData).toBe(null);
    });

    test('should return default value for non-existent files', () => {
      const defaultValue = { default: true };
      
      const loadedData = storage.load('non-existent.json', { defaultValue });
      
      expect(loadedData).toEqual(defaultValue);
    });

    test('should convert date fields correctly', () => {
      const testData = {
        createdAt: '2025-01-03T10:00:00.000Z',
        updatedAt: '2025-01-03T11:00:00.000Z',
        nested: {
          startTime: '2025-01-03T12:00:00.000Z'
        },
        normalField: 'not a date'
      };
      
      // Save data manually (without conversion)
      fs.writeFileSync(storage.getFilePath(TEST_FILE), JSON.stringify(testData));
      
      // Load with date conversion
      const loadedData = storage.load(TEST_FILE, {
        dateFields: ['createdAt', 'updatedAt', 'startTime']
      });
      
      expect(loadedData.createdAt).toBeInstanceOf(Date);
      expect(loadedData.updatedAt).toBeInstanceOf(Date);
      expect(loadedData.nested.startTime).toBeInstanceOf(Date);
      expect(loadedData.normalField).toBe('not a date');
    });

    test('should handle malformed JSON gracefully', () => {
      // Write invalid JSON
      fs.writeFileSync(storage.getFilePath(TEST_FILE), '{ invalid json }');
      
      const loadedData = storage.load(TEST_FILE);
      
      expect(loadedData).toBe(null);
    });

    test('should handle date conversion errors gracefully', () => {
      const testData = {
        invalidDate: 'not-a-date-string',
        validDate: '2025-01-03T10:00:00.000Z'
      };
      
      fs.writeFileSync(storage.getFilePath(TEST_FILE), JSON.stringify(testData));
      
      const loadedData = storage.load(TEST_FILE, {
        dateFields: ['invalidDate', 'validDate']
      });
      
      expect(loadedData.invalidDate).toBe('not-a-date-string'); // Should remain string
      expect(loadedData.validDate).toBeInstanceOf(Date); // Should be converted
    });
  });

  describe('Update Operations', () => {
    test('should update existing data', () => {
      const initialData = { count: 1, message: 'initial' };
      
      // Save initial data
      storage.save(TEST_FILE, initialData);
      
      // Update data
      const result = storage.update(TEST_FILE, (data) => ({
        ...data,
        count: data.count + 1,
        message: 'updated'
      }));
      
      expect(result).toBe(true);
      
      // Verify update
      const updatedData = storage.load(TEST_FILE);
      expect(updatedData.count).toBe(2);
      expect(updatedData.message).toBe('updated');
    });

    test('should handle non-existent files in update', () => {
      const result = storage.update('non-existent.json', (data) => ({
        ...data,
        newField: 'created'
      }));
      
      expect(result).toBe(true);
      
      // Verify file was created
      const newData = storage.load('non-existent.json');
      expect(newData.newField).toBe('created');
    });

    test('should handle update function errors', () => {
      storage.save(TEST_FILE, { count: 1 });
      
      const result = storage.update(TEST_FILE, () => {
        throw new Error('Update function error');
      });
      
      expect(result).toBe(false);
    });
  });

  describe('Delete Operations', () => {
    test('should delete existing files', () => {
      // Create file first
      storage.save(TEST_FILE, { test: 'data' });
      expect(fs.existsSync(storage.getFilePath(TEST_FILE))).toBe(true);
      
      // Delete file
      const result = storage.delete(TEST_FILE);
      
      expect(result).toBe(true);
      expect(fs.existsSync(storage.getFilePath(TEST_FILE))).toBe(false);
    });

    test('should handle non-existent files gracefully', () => {
      const result = storage.delete('non-existent.json');
      
      expect(result).toBe(true); // Should return true even if file doesn't exist
    });
  });

  describe('File Listing', () => {
    test('should list JSON files', () => {
      // Create test files
      storage.save('file1.json', { data: 1 });
      storage.save('file2.json', { data: 2 });
      fs.writeFileSync(path.join(TEST_DIR, 'not-json.txt'), 'text file');
      
      const files = storage.listFiles();
      
      expect(files).toContain('file1.json');
      expect(files).toContain('file2.json');
      expect(files).not.toContain('not-json.txt');
      expect(files.length).toBe(2);
    });

    test('should return empty array for non-existent directory', () => {
      const invalidStorage = new PersistentStorage('/non-existent-path');
      
      const files = invalidStorage.listFiles();
      
      expect(files).toEqual([]);
    });
  });

  describe('Storage Statistics', () => {
    test('should return accurate storage statistics', () => {
      // Create test files
      storage.save('small.json', { data: 'small' });
      storage.save('large.json', { data: 'large'.repeat(100) });
      
      const stats = storage.getStats();
      
      expect(stats.totalFiles).toBe(2);
      expect(stats.files).toHaveLength(2);
      expect(stats.totalSize).toBeGreaterThan(0);
      
      const smallFile = stats.files.find(f => f.name === 'small.json');
      const largeFile = stats.files.find(f => f.name === 'large.json');
      
      expect(smallFile).toBeDefined();
      expect(largeFile).toBeDefined();
      expect(largeFile.size).toBeGreaterThan(smallFile.size);
      expect(smallFile.modified).toBeInstanceOf(Date);
    });

    test('should handle statistics errors gracefully', () => {
      const invalidStorage = new PersistentStorage('/non-existent-path');
      
      const stats = invalidStorage.getStats();
      
      expect(stats.totalFiles).toBe(0);
      expect(stats.files).toEqual([]);
      expect(stats.totalSize).toBe(0);
    });
  });

  describe('Date Conversion Utility', () => {
    test('should convert dates recursively in nested objects', () => {
      const testObj = {
        topLevel: '2025-01-03T10:00:00.000Z',
        nested: {
          deepLevel: '2025-01-03T11:00:00.000Z',
          array: [
            { itemDate: '2025-01-03T12:00:00.000Z' }
          ]
        },
        notADate: 'regular string'
      };
      
      storage.convertDatesRecursive(testObj, ['topLevel', 'deepLevel', 'itemDate']);
      
      expect(testObj.topLevel).toBeInstanceOf(Date);
      expect(testObj.nested.deepLevel).toBeInstanceOf(Date);
      expect(testObj.nested.array[0].itemDate).toBeInstanceOf(Date);
      expect(testObj.notADate).toBe('regular string');
    });

    test('should handle non-object inputs gracefully', () => {
      expect(() => {
        storage.convertDatesRecursive(null, ['date']);
      }).not.toThrow();
      
      expect(() => {
        storage.convertDatesRecursive('string', ['date']);
      }).not.toThrow();
      
      expect(() => {
        storage.convertDatesRecursive(42, ['date']);
      }).not.toThrow();
    });
  });

  describe('Integration Scenarios', () => {
    test('should handle Academy-like data structures', () => {
      const academyData = {
        completedPersonas: {
          'TestPersona-123': {
            personaName: 'TestPersona-123',
            status: 'failed',
            startTime: '2025-01-03T10:00:00.000Z',
            completedAt: '2025-01-03T10:30:00.000Z',
            graduationScore: 0.67,
            logs: ['Training started', 'Round 1 completed']
          }
        }
      };
      
      // Save Academy data
      const saveResult = storage.save('academy-test.json', academyData, {
        source: 'AcademyWebInterface'
      });
      expect(saveResult).toBe(true);
      
      // Load Academy data with date conversion
      const loadedData = storage.load('academy-test.json', {
        dateFields: ['startTime', 'completedAt']
      });
      
      expect(loadedData.completedPersonas['TestPersona-123'].startTime).toBeInstanceOf(Date);
      expect(loadedData.completedPersonas['TestPersona-123'].completedAt).toBeInstanceOf(Date);
      expect(loadedData.completedPersonas['TestPersona-123'].graduationScore).toBe(0.67);
      expect(loadedData._metadata.source).toBe('AcademyWebInterface');
    });

    test('should handle multiple save/load cycles', () => {
      let data = { version: 1, items: [] };
      
      // Multiple save/load cycles
      for (let i = 0; i < 5; i++) {
        data.items.push(`item-${i}`);
        data.version++;
        
        storage.save('cycle-test.json', data);
        data = storage.load('cycle-test.json');
      }
      
      expect(data.version).toBe(6);
      expect(data.items).toHaveLength(5);
      expect(data.items[4]).toBe('item-4');
    });
  });
});