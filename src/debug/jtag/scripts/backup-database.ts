#!/usr/bin/env tsx
/**
 * Database Backup Script
 *
 * Backs up the main database to the backup directory.
 * Paths are read from config.env via PATHS constants.
 */

import * as fs from 'fs/promises';
import * as fssync from 'fs';
import * as path from 'path';
import * as os from 'os';

// Load config.env before importing PATHS
const configPath = path.join(os.homedir(), '.continuum', 'config.env');
if (fssync.existsSync(configPath)) {
  const content = fssync.readFileSync(configPath, 'utf-8');
  for (const line of content.split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const match = trimmed.match(/^([A-Z_0-9]+)\s*=\s*(.*)$/);
    if (match) {
      const [, key, rawValue] = match;
      let value = rawValue.trim();
      if (value.startsWith('~/')) {
        value = path.join(os.homedir(), value.slice(2));
      }
      if (key.startsWith('DATABASE_') || key.startsWith('DATASETS_')) {
        process.env[key] = value;
      }
    }
  }
}

import { PATHS } from '../system/shared/Constants';

async function backupDatabase() {
  try {
    const dbPath = PATHS.SQLITE_DB;
    const backupDir = PATHS.DATABASE_BACKUP_DIR;
    const timestamp = new Date().toISOString().replace(/[:.]/g, '-').split('T').join('T').slice(0, -5);
    const backupPath = path.join(backupDir, `database-backup-${timestamp}.sqlite`);

    // Create backup directory if it doesn't exist
    await fs.mkdir(backupDir, { recursive: true });

    // Check if database exists
    try {
      await fs.access(dbPath);
    } catch (error) {
      console.error(`❌ Database not found at: ${dbPath}`);
      process.exit(1);
    }

    // Copy database to backup location
    await fs.copyFile(dbPath, backupPath);

    console.log(`✅ Backed up to ${backupPath}`);

    // List recent backups
    const files = await fs.readdir(backupDir);
    const sqliteFiles = files.filter(f => f.endsWith('.sqlite'))
      .map(f => path.join(backupDir, f));

    const stats = await Promise.all(
      sqliteFiles.map(async f => ({
        path: f,
        stat: await fs.stat(f)
      }))
    );

    const recent = stats
      .sort((a, b) => b.stat.mtime.getTime() - a.stat.mtime.getTime())
      .slice(0, 5);

    console.log('\nRecent backups:');
    for (const { path: p, stat } of recent) {
      const size = (stat.size / 1024 / 1024).toFixed(2);
      console.log(`  ${path.basename(p)} (${size} MB)`);
    }

  } catch (error) {
    console.error('❌ Backup failed:', error);
    process.exit(1);
  }
}

backupDatabase();
