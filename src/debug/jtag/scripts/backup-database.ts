#!/usr/bin/env tsx
/**
 * Database Backup Script
 *
 * Creates a timestamped backup of the SQLite database.
 * This PREVENTS CATASTROPHIC DATA LOSS from accidental data:reseed/data:clear.
 *
 * Usage:
 *   npm run data:backup
 *   npx tsx scripts/backup-database.ts
 */

import * as fs from 'fs';
import * as path from 'path';
import { PATHS } from '../system/shared/Constants';

async function backupDatabase(): Promise<void> {
  const dbPath = path.join(process.cwd(), PATHS.SQLITE_DB);
  const backupDir = path.join(process.cwd(), PATHS.JTAG_BACKUPS);

  // Check if database exists
  if (!fs.existsSync(dbPath)) {
    console.log('ℹ️ No database found at:', dbPath);
    console.log('   This is normal if you haven\'t started the system yet.');
    return;
  }

  // Create backup directory if it doesn't exist
  if (!fs.existsSync(backupDir)) {
    fs.mkdirSync(backupDir, { recursive: true });
    console.log('📁 Created backup directory:', backupDir);
  }

  // Create timestamped backup filename
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5); // 2025-11-19T01-46-12
  const backupFilename = `database-backup-${timestamp}.sqlite`;
  const backupPath = path.join(backupDir, backupFilename);

  // Copy database to backup
  try {
    fs.copyFileSync(dbPath, backupPath);
    const stats = fs.statSync(backupPath);
    const sizeMB = (stats.size / (1024 * 1024)).toFixed(2);

    console.log('✅ Database backed up successfully!');
    console.log(`   Source: ${dbPath}`);
    console.log(`   Backup: ${backupPath}`);
    console.log(`   Size: ${sizeMB} MB`);
    console.log(`   Timestamp: ${timestamp}`);

    // List all backups
    const backups = fs.readdirSync(backupDir)
      .filter(f => f.startsWith('database-backup-') && f.endsWith('.sqlite'))
      .sort()
      .reverse();

    if (backups.length > 1) {
      console.log(`\n📋 Total backups: ${backups.length}`);
      console.log('   Most recent backups:');
      backups.slice(0, 5).forEach((backup, i) => {
        const backupStat = fs.statSync(path.join(backupDir, backup));
        const backupSizeMB = (backupStat.size / (1024 * 1024)).toFixed(2);
        console.log(`   ${i + 1}. ${backup} (${backupSizeMB} MB)`);
      });
    }

    // Clean up old backups (keep last 10)
    if (backups.length > 10) {
      const toDelete = backups.slice(10);
      console.log(`\n🗑️ Removing ${toDelete.length} old backups (keeping most recent 10)...`);
      toDelete.forEach(backup => {
        fs.unlinkSync(path.join(backupDir, backup));
        console.log(`   Deleted: ${backup}`);
      });
    }

  } catch (error) {
    console.error('❌ Backup failed:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  backupDatabase().catch(console.error);
}

export { backupDatabase };
