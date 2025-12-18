#!/usr/bin/env tsx
/**
 * Database Restore Script
 *
 * Lists available backups and shows restore instructions.
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

async function showRestoreInfo() {
  try {
    const dbPath = PATHS.SQLITE_DB;
    const backupDir = PATHS.DATABASE_BACKUP_DIR;

    // Check if backup directory exists
    try {
      await fs.access(backupDir);
    } catch (error) {
      console.error(`❌ Backup directory not found: ${backupDir}`);
      process.exit(1);
    }

    // List all backup files
    const files = await fs.readdir(backupDir);
    const sqliteFiles = files.filter(f => f.endsWith('.sqlite'))
      .map(f => path.join(backupDir, f));

    if (sqliteFiles.length === 0) {
      console.log('❌ No backups found');
      process.exit(1);
    }

    const stats = await Promise.all(
      sqliteFiles.map(async f => ({
        path: f,
        stat: await fs.stat(f)
      }))
    );

    const sorted = stats.sort((a, b) => b.stat.mtime.getTime() - a.stat.mtime.getTime());

    console.log('📦 Available backups:\n');
    for (const { path: p, stat } of sorted.slice(0, 10)) {
      const size = (stat.size / 1024 / 1024).toFixed(2);
      const date = stat.mtime.toISOString();
      console.log(`  ${path.basename(p)}`);
      console.log(`    Size: ${size} MB | Date: ${date}\n`);
    }

    console.log('\n📖 To restore a backup:\n');
    console.log(`  1. Stop the system: npm stop`);
    console.log(`  2. Copy backup: cp "${backupDir}/<backup-file>" "${dbPath}"`);
    console.log(`  3. Start the system: npm start\n`);

  } catch (error) {
    console.error('❌ Failed to list backups:', error);
    process.exit(1);
  }
}

showRestoreInfo();
