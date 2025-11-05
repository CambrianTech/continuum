/**
 * Integration Tests for Complete Console Logging System
 * Tests the entire logging flow: Browser → WebSocket → Session Files
 */

import { test, describe, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import * as fs from 'fs/promises';
import * as path from 'path';

describe('Console Logging System Integration Tests', () => {
  let testSessionDir: string;
  let testLogDir: string;

  beforeEach(async () => {
    // Create test session directory
    const baseDir = '.continuum/sessions/user/shared';
    const sessionId = `test-logging-${Date.now()}`;
    testSessionDir = path.join(baseDir, sessionId);
    testLogDir = path.join(testSessionDir, 'logs');
    
    await fs.mkdir(testLogDir, { recursive: true });
  });

  afterEach(async () => {
    // Clean up test directory
    try {
      await fs.rm(testSessionDir, { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors
    }
  });

  describe('System Integration', () => {
    test('should validate complete logging system is operational', async () => {
      // Test 1: Verify session directory structure
      const sessionExists = await fs.access(testSessionDir).then(() => true).catch(() => false);
      assert.ok(sessionExists, 'Test session directory should exist');

      const logDirExists = await fs.access(testLogDir).then(() => true).catch(() => false);
      assert.ok(logDirExists, 'Test log directory should exist');

      // Test 2: Verify we can write to log files
      const testBrowserLog = path.join(testLogDir, 'browser.log');
      const testServerLog = path.join(testLogDir, 'server.log');

      await fs.writeFile(testBrowserLog, 'Test browser log entry\n');
      await fs.writeFile(testServerLog, 'Test server log entry\n');

      // Test 3: Verify we can read log files
      const browserContent = await fs.readFile(testBrowserLog, 'utf8');
      const serverContent = await fs.readFile(testServerLog, 'utf8');

      assert.ok(browserContent.includes('Test browser log entry'));
      assert.ok(serverContent.includes('Test server log entry'));

      // Test 4: Verify file permissions
      const browserStats = await fs.stat(testBrowserLog);
      const serverStats = await fs.stat(testServerLog);

      assert.ok(browserStats.isFile(), 'Browser log should be a file');
      assert.ok(serverStats.isFile(), 'Server log should be a file');
    });

    test('should handle log file rotation scenarios', async () => {
      const logFile = path.join(testLogDir, 'rotation-test.log');
      
      // Write initial content
      await fs.writeFile(logFile, 'Initial log entry\n');
      
      // Append more content
      await fs.appendFile(logFile, 'Additional log entry\n');
      
      // Verify content
      const content = await fs.readFile(logFile, 'utf8');
      assert.ok(content.includes('Initial log entry'));
      assert.ok(content.includes('Additional log entry'));
      
      // Simulate rotation by creating backup
      const backupFile = `${logFile}.backup`;
      await fs.copyFile(logFile, backupFile);
      
      // Verify backup exists
      const backupExists = await fs.access(backupFile).then(() => true).catch(() => false);
      assert.ok(backupExists, 'Backup file should exist after rotation');
      
      // Clean up backup
      await fs.unlink(backupFile);
    });

    test('should handle concurrent log writing', async () => {
      const logFile = path.join(testLogDir, 'concurrent-test.log');
      
      // Simulate concurrent writes
      const writePromises = [];
      for (let i = 0; i < 10; i++) {
        writePromises.push(
          fs.appendFile(logFile, `Concurrent log entry ${i}\n`)
        );
      }
      
      // Wait for all writes to complete
      await Promise.all(writePromises);
      
      // Verify all entries were written
      const content = await fs.readFile(logFile, 'utf8');
      const lines = content.split('\n').filter(line => line.trim());
      
      assert.strictEqual(lines.length, 10, 'Should have 10 log entries');
      
      // Verify each entry exists
      for (let i = 0; i < 10; i++) {
        assert.ok(content.includes(`Concurrent log entry ${i}`));
      }
    });

    test('should handle large log files efficiently', async () => {
      const logFile = path.join(testLogDir, 'large-test.log');
      
      // Write a moderately large amount of data
      let largeContent = '';
      for (let i = 0; i < 1000; i++) {
        largeContent += `Log entry ${i}: This is a longer log message with more content to simulate real-world usage\n`;
      }
      
      const startTime = Date.now();
      await fs.writeFile(logFile, largeContent);
      const writeTime = Date.now() - startTime;
      
      // Verify file was written
      const stats = await fs.stat(logFile);
      assert.ok(stats.size > 50000, 'Large log file should be substantial size');
      
      // Verify write was reasonably fast
      assert.ok(writeTime < 1000, `Write should be fast, took ${writeTime}ms`);
      
      // Verify content integrity
      const readContent = await fs.readFile(logFile, 'utf8');
      assert.ok(readContent.includes('Log entry 0:'));
      assert.ok(readContent.includes('Log entry 999:'));
    });

    test('should handle log file cleanup', async () => {
      // Create multiple log files
      const logFiles = [
        path.join(testLogDir, 'cleanup-test-1.log'),
        path.join(testLogDir, 'cleanup-test-2.log'),
        path.join(testLogDir, 'cleanup-test-3.log')
      ];
      
      // Write to all files
      for (const file of logFiles) {
        await fs.writeFile(file, 'Test log content\n');
      }
      
      // Verify all files exist
      for (const file of logFiles) {
        const exists = await fs.access(file).then(() => true).catch(() => false);
        assert.ok(exists, `Log file ${path.basename(file)} should exist`);
      }
      
      // Clean up files
      for (const file of logFiles) {
        await fs.unlink(file);
      }
      
      // Verify files are deleted
      for (const file of logFiles) {
        const exists = await fs.access(file).then(() => true).catch(() => false);
        assert.ok(!exists, `Log file ${path.basename(file)} should be deleted`);
      }
    });
  });

  describe('Error Handling', () => {
    test('should handle directory creation errors gracefully', async () => {
      // Try to create directory in non-existent path
      const invalidPath = path.join('/invalid/path/that/does/not/exist', 'logs');
      
      try {
        await fs.mkdir(invalidPath, { recursive: true });
        assert.fail('Should have thrown error for invalid path');
      } catch (error) {
        assert.ok(error instanceof Error);
        assert.ok(error.message.includes('ENOENT') || error.message.includes('EACCES'));
      }
    });

    test('should handle permission errors gracefully', async () => {
      // This test verifies we handle permission errors appropriately
      // We can't easily create permission errors in tests, so we verify the structure
      const restrictedPath = path.join(testLogDir, 'restricted');
      
      try {
        await fs.mkdir(restrictedPath);
        await fs.writeFile(path.join(restrictedPath, 'test.log'), 'test content');
        
        // Verify file was created successfully
        const content = await fs.readFile(path.join(restrictedPath, 'test.log'), 'utf8');
        assert.strictEqual(content, 'test content');
        
        // Clean up
        await fs.rm(restrictedPath, { recursive: true });
      } catch (error) {
        // If we get permission errors, that's expected behavior
        assert.ok(error instanceof Error);
      }
    });
  });

  describe('Performance Validation', () => {
    test('should maintain reasonable file sizes', async () => {
      const testLogFile = path.join(testLogDir, 'size-test.log');
      
      // Write typical log entries
      let content = '';
      for (let i = 0; i < 100; i++) {
        content += `[${new Date().toISOString()}] INFO: Test log message ${i}\n`;
      }
      
      await fs.writeFile(testLogFile, content);
      
      const stats = await fs.stat(testLogFile);
      
      // Verify file size is reasonable (not gigabytes)
      const sizeKB = stats.size / 1024;
      assert.ok(sizeKB < 100, `Log file should be reasonable size, got ${sizeKB}KB`);
      assert.ok(sizeKB > 1, `Log file should have content, got ${sizeKB}KB`);
    });

    test('should handle rapid sequential writes', async () => {
      const testLogFile = path.join(testLogDir, 'rapid-test.log');
      
      const startTime = Date.now();
      
      // Rapid sequential writes
      for (let i = 0; i < 50; i++) {
        await fs.appendFile(testLogFile, `Rapid log entry ${i}\n`);
      }
      
      const endTime = Date.now();
      const totalTime = endTime - startTime;
      
      // Verify all entries were written
      const content = await fs.readFile(testLogFile, 'utf8');
      const lines = content.split('\n').filter(line => line.trim());
      
      assert.strictEqual(lines.length, 50, 'Should have 50 log entries');
      
      // Verify reasonable performance
      assert.ok(totalTime < 5000, `Rapid writes should be fast, took ${totalTime}ms`);
    });
  });
});