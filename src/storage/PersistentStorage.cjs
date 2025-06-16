/**
 * Persistent Storage Manager
 * Handles saving and loading data to/from JSON files with proper error handling
 */

const fs = require('fs');
const path = require('path');

class PersistentStorage {
  constructor(baseDir = '.continuum') {
    this.baseDir = baseDir;
    this.ensureDirectoryExists();
  }

  /**
   * Ensure the base directory exists
   */
  ensureDirectoryExists() {
    if (!fs.existsSync(this.baseDir)) {
      fs.mkdirSync(this.baseDir, { recursive: true });
      console.log(`📁 Created storage directory: ${this.baseDir}`);
    }
  }

  /**
   * Get the full path for a storage file
   */
  getFilePath(filename) {
    return path.join(this.baseDir, filename);
  }

  /**
   * Save data to a JSON file
   * @param {string} filename - Name of the file (e.g., 'academy-sessions.json')
   * @param {Object} data - Data to save
   * @param {Object} options - Options for saving
   * @returns {boolean} - Success status
   */
  save(filename, data, options = {}) {
    try {
      const filePath = this.getFilePath(filename);
      
      // Add metadata
      const saveData = {
        ...data,
        _metadata: {
          savedAt: new Date().toISOString(),
          version: options.version || '1.0.0',
          source: options.source || 'PersistentStorage'
        }
      };

      // Write with pretty formatting
      fs.writeFileSync(filePath, JSON.stringify(saveData, null, 2));
      
      if (options.verbose !== false) {
        console.log(`💾 Saved data to ${filename}`);
      }
      
      return true;
    } catch (error) {
      console.error(`❌ Failed to save ${filename}:`, error.message);
      return false;
    }
  }

  /**
   * Load data from a JSON file
   * @param {string} filename - Name of the file to load
   * @param {Object} options - Options for loading
   * @returns {Object|null} - Loaded data or null if failed
   */
  load(filename, options = {}) {
    try {
      const filePath = this.getFilePath(filename);
      
      if (!fs.existsSync(filePath)) {
        if (options.verbose !== false) {
          console.log(`📄 File ${filename} does not exist, returning empty data`);
        }
        return options.defaultValue || null;
      }

      const rawData = fs.readFileSync(filePath, 'utf8');
      const data = JSON.parse(rawData);
      
      // Convert date strings back to Date objects if specified
      if (options.dateFields && Array.isArray(options.dateFields)) {
        this.convertDatesRecursive(data, options.dateFields);
      }
      
      if (options.verbose !== false) {
        console.log(`📖 Loaded data from ${filename}`);
      }
      
      return data;
    } catch (error) {
      console.error(`❌ Failed to load ${filename}:`, error.message);
      return options.defaultValue || null;
    }
  }

  /**
   * Update data in a file (load, modify, save)
   * @param {string} filename - Name of the file
   * @param {Function} updateFn - Function that receives current data and returns updated data
   * @param {Object} options - Options for the operation
   * @returns {boolean} - Success status
   */
  update(filename, updateFn, options = {}) {
    try {
      const currentData = this.load(filename, { ...options, verbose: false });
      const updatedData = updateFn(currentData || {});
      return this.save(filename, updatedData, { ...options, verbose: false });
    } catch (error) {
      console.error(`❌ Failed to update ${filename}:`, error.message);
      return false;
    }
  }

  /**
   * Delete a storage file
   * @param {string} filename - Name of the file to delete
   * @returns {boolean} - Success status
   */
  delete(filename) {
    try {
      const filePath = this.getFilePath(filename);
      
      if (fs.existsSync(filePath)) {
        fs.unlinkSync(filePath);
        console.log(`🗑️ Deleted ${filename}`);
        return true;
      }
      
      console.log(`📄 File ${filename} does not exist, nothing to delete`);
      return true;
    } catch (error) {
      console.error(`❌ Failed to delete ${filename}:`, error.message);
      return false;
    }
  }

  /**
   * List all files in the storage directory
   * @returns {Array} - Array of filenames
   */
  listFiles() {
    try {
      return fs.readdirSync(this.baseDir).filter(file => file.endsWith('.json'));
    } catch (error) {
      console.error('❌ Failed to list storage files:', error.message);
      return [];
    }
  }

  /**
   * Convert date strings to Date objects recursively
   * @param {Object} obj - Object to process
   * @param {Array} dateFields - Array of field names that should be converted to dates
   */
  convertDatesRecursive(obj, dateFields) {
    if (!obj || typeof obj !== 'object') return;

    for (const [key, value] of Object.entries(obj)) {
      if (dateFields.includes(key) && typeof value === 'string') {
        try {
          const date = new Date(value);
          // Only convert if the date is valid
          if (!isNaN(date.getTime())) {
            obj[key] = date;
          }
        } catch (error) {
          console.warn(`⚠️ Failed to convert ${key} to Date:`, error.message);
        }
      } else if (typeof value === 'object' && value !== null) {
        this.convertDatesRecursive(value, dateFields);
      }
    }
  }

  /**
   * Get storage statistics
   * @returns {Object} - Storage statistics
   */
  getStats() {
    try {
      const files = this.listFiles();
      const stats = {
        totalFiles: files.length,
        files: [],
        totalSize: 0
      };

      files.forEach(filename => {
        const filePath = this.getFilePath(filename);
        const stat = fs.statSync(filePath);
        stats.files.push({
          name: filename,
          size: stat.size,
          modified: stat.mtime
        });
        stats.totalSize += stat.size;
      });

      return stats;
    } catch (error) {
      console.error('❌ Failed to get storage stats:', error.message);
      return { totalFiles: 0, files: [], totalSize: 0 };
    }
  }
}

module.exports = PersistentStorage;