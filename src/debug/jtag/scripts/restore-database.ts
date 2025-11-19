#!/usr/bin/env tsx
/**
 * Database Restore Script
 *
 * Restores database from a backup file.
 * Use this to recover from accidental data:reseed/data:clear.
 *
 * Usage:
 *   npm run data:restore                    # Interactive: shows list of backups
 *   npm run data:restore -- <backup-name>   # Direct: restore specific backup
 *   npx tsx scripts/restore-database.ts database-backup-2025-11-19T01-46-12.sqlite
 */

import * as fs from 'fs';
import * as path from 'path';
import * as readline from 'readline';
import { PATHS } from '../system/shared/Constants';

async function askQuestion(query: string): Promise<string> {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout
  });

  return new Promise(resolve => {
    rl.question(query, (answer: string) => {
      rl.close();
      resolve(answer);
    });
  });
}

async function restoreDatabase(backupFilename?: string): Promise<void> {
  const dbPath = path.join(process.cwd(), PATHS.SQLITE_DB);
  const backupDir = path.join(process.cwd(), PATHS.JTAG_BACKUPS);

  // Check if backup directory exists
  if (!fs.existsSync(backupDir)) {
    console.error('❌ No backup directory found:', backupDir);
    console.log('   Run "npm run data:backup" first to create backups.');
    process.exit(1);
  }

  // List available backups
  const backups = fs.readdirSync(backupDir)
    .filter(f => f.startsWith('database-backup-') && f.endsWith('.sqlite'))
    .sort()
    .reverse();

  if (backups.length === 0) {
    console.error('❌ No backups found in:', backupDir);
    console.log('   Run "npm run data:backup" first to create backups.');
    process.exit(1);
  }

  console.log('📋 Available backups:');
  backups.forEach((backup, i) => {
    const backupPath = path.join(backupDir, backup);
    const stats = fs.statSync(backupPath);
    const sizeMB = (stats.size / (1024 * 1024)).toFixed(2);
    const date = new Date(stats.mtime).toLocaleString();
    console.log(`   ${i + 1}. ${backup}`);
    console.log(`      Size: ${sizeMB} MB, Modified: ${date}`);
  });

  let selectedBackup: string;

  if (backupFilename) {
    // Direct restore from command line argument
    if (!backups.includes(backupFilename)) {
      console.error(`❌ Backup not found: ${backupFilename}`);
      console.log('   Available backups listed above.');
      process.exit(1);
    }
    selectedBackup = backupFilename;
  } else {
    // Interactive selection
    console.log('\n🔄 Select backup to restore:');
    const answer = await askQuestion('Enter number (1-' + backups.length + ') or filename: ');

    if (answer.match(/^\d+$/)) {
      const index = parseInt(answer, 10) - 1;
      if (index < 0 || index >= backups.length) {
        console.error('❌ Invalid selection:', answer);
        process.exit(1);
      }
      selectedBackup = backups[index];
    } else {
      if (!backups.includes(answer)) {
        console.error(`❌ Backup not found: ${answer}`);
        process.exit(1);
      }
      selectedBackup = answer;
    }
  }

  const backupPath = path.join(backupDir, selectedBackup);

  // Confirm restore
  console.log(`\n⚠️  WARNING: This will replace the current database!`);
  console.log(`   Current: ${dbPath}`);
  console.log(`   Restore from: ${backupPath}`);

  const confirm = await askQuestion('\nType "yes" to confirm: ');
  if (confirm.toLowerCase() !== 'yes') {
    console.log('❌ Restore cancelled.');
    process.exit(0);
  }

  // Backup current database before overwriting (just in case)
  if (fs.existsSync(dbPath)) {
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, -5);
    const preRestoreBackup = path.join(backupDir, `database-pre-restore-${timestamp}.sqlite`);
    fs.copyFileSync(dbPath, preRestoreBackup);
    console.log('💾 Created safety backup of current database:', path.basename(preRestoreBackup));
  }

  // Restore from backup
  try {
    fs.copyFileSync(backupPath, dbPath);
    const stats = fs.statSync(dbPath);
    const sizeMB = (stats.size / (1024 * 1024)).toFixed(2);

    console.log('✅ Database restored successfully!');
    console.log(`   From: ${backupPath}`);
    console.log(`   To: ${dbPath}`);
    console.log(`   Size: ${sizeMB} MB`);
    console.log('\n🔄 Restart the system to load restored data: npm start');

  } catch (error) {
    console.error('❌ Restore failed:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  const backupFilename = process.argv[2]; // Optional: backup filename from command line
  restoreDatabase(backupFilename).catch(console.error);
}

export { restoreDatabase };
