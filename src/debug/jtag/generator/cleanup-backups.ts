#!/usr/bin/env ts-node
/**
 * Backup File Cleanup Utility
 * 
 * Removes all .backup-* files from the JTAG directory to fix backup pollution.
 * Run this script to clean up the dozens of backup files created by the generator.
 */

import { FileManager } from './utils/FileManager';
import { ConsoleLogger } from './utils/Logger';
import { join } from 'path';

async function cleanupBackupPollution(): Promise<void> {
  const logger = new ConsoleLogger('info');
  const fileManager = new FileManager(logger);

  logger.info('🧹 Starting backup file cleanup...');
  
  // Clean up backup files in the root directory
  fileManager.cleanupAllBackups(process.cwd());
  
  // Also check and clean other directories that might have backups
  const directoriesToCheck = [
    '.continuum/generator',
    'generator',
    'scripts',
    'dist'
  ];
  
  for (const dir of directoriesToCheck) {
    const dirPath = join(process.cwd(), dir);
    if (fileManager.exists(dirPath)) {
      logger.info(`🔍 Checking ${dir} for backup files...`);
      fileManager.cleanupAllBackups(dirPath);
    }
  }
  
  logger.info('✅ Backup cleanup complete!');
}

// CLI execution
if (require.main === module) {
  cleanupBackupPollution().catch(error => {
    console.error('💥 Backup cleanup failed:', error);
    process.exit(1);
  });
}