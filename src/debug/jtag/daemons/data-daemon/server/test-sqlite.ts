/**
 * Standalone SQLite test to isolate READONLY issue
 */
import sqlite3 from 'sqlite3';
import * as fs from 'fs/promises';
import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

async function main() {
  const dbPath = '.continuum/jtag/data/test-database.sqlite';

  console.log('🧪 SQLite READONLY Test');
  console.log('========================\n');

  // Ensure directory exists
  await fs.mkdir('.continuum/jtag/data', { recursive: true });

  // Remove old test database
  try {
    await fs.unlink(dbPath);
    console.log('✅ Removed old test database');
  } catch (e) {
    // File doesn't exist, that's fine
  }

  // Set permissions and clear xattr BEFORE opening connection
  console.log('\n📁 Pre-connection cleanup:');
  try {
    // Create empty file
    await fs.writeFile(dbPath, '');
    console.log('  Created empty file');

    await fs.chmod(dbPath, 0o666);
    console.log('  Set permissions to 0666');

    if (process.platform === 'darwin') {
      await execAsync(`xattr -c "${dbPath}"`);
      console.log('  Cleared macOS extended attributes');
    }
  } catch (error) {
    console.warn('  Warning during pre-setup:', error);
  }

  // Open database
  console.log('\n🔗 Opening database connection...');
  const db = new sqlite3.Database(dbPath, sqlite3.OPEN_READWRITE | sqlite3.OPEN_CREATE, (err) => {
    if (err) {
      console.error('❌ Failed to open:', err);
      process.exit(1);
    }
    console.log('✅ Database opened');
  });

  // Helper to run SQL
  const runSql = (sql: string): Promise<void> => {
    return new Promise((resolve, reject) => {
      db.run(sql, (err) => {
        if (err) reject(err);
        else resolve();
      });
    });
  };

  try {
    console.log('\n🏗️  Test 1: CREATE TABLE');
    await runSql(`
      CREATE TABLE IF NOT EXISTS test_users (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        created_at TEXT DEFAULT CURRENT_TIMESTAMP
      )
    `);
    console.log('✅ Table created successfully');

    console.log('\n🏗️  Test 2: CREATE INDEX');
    await runSql(`CREATE INDEX IF NOT EXISTS idx_test_users_name ON test_users(name)`);
    console.log('✅ Index created successfully');

    console.log('\n📝 Test 3: INSERT');
    await runSql(`INSERT INTO test_users (id, name) VALUES ('test-1', 'Test User')`);
    console.log('✅ Insert successful');

    console.log('\n🎉 ALL TESTS PASSED!');

  } catch (error: any) {
    console.error('\n❌ TEST FAILED:');
    console.error('  Error:', error.message);
    console.error('  Code:', error.code);
    console.error('  Errno:', error.errno);
  } finally {
    db.close();
  }
}

main().catch(console.error);
