#!/usr/bin/env npx tsx
/**
 * Migration Script: Move large ragContext from SQLite to BlobStorage
 *
 * Problem: coordination_decisions table is 262MB with only 3,480 rows
 * because ragContext stores full RAG context (avg 73KB per row).
 *
 * Solution: Move large ragContext to content-addressable blob storage
 * and replace with ragContextRef (sha256 hash pointer).
 *
 * ARCHITECTURE: Uses Commands.execute via JTAG CLI (no direct database access)
 * This ensures proper integration with the data daemon adapter architecture.
 *
 * PREREQUISITE: JTAG system must be running (`npm start`)
 *
 * Usage:
 *   npx tsx scripts/migrate-rag-context-to-blobs.ts [--dry-run] [--batch-size=100]
 *
 * Options:
 *   --dry-run      Show what would be migrated without actually doing it
 *   --batch-size   Number of records to process at a time (default: 100)
 */

import { execSync } from 'child_process';
import { BlobStorage } from '../system/storage/BlobStorage';

const THRESHOLD_BYTES = 4096; // 4KB - same as BlobStorage default

interface MigrationStats {
  totalRecords: number;
  alreadyMigrated: number;  // Has ragContextRef
  belowThreshold: number;   // Small enough to stay inline
  migrated: number;         // Successfully moved to blob
  errors: number;
  totalSavedBytes: number;
}

interface CoordinationDecisionRecord {
  id: string;
  ragContext?: string | Record<string, unknown>;
  ragContextRef?: string;
}

/**
 * Execute a JTAG command and parse JSON response
 * Uses the running JTAG server via CLI
 */
function executeJtag<T>(command: string): T {
  try {
    const output = execSync(`./jtag ${command}`, {
      encoding: 'utf8',
      cwd: process.cwd(),
      timeout: 60000, // 60 second timeout for large queries
      stdio: ['pipe', 'pipe', 'pipe']
    });

    // Find JSON in output (jtag may include other output)
    const jsonStart = output.lastIndexOf('{');
    if (jsonStart < 0) {
      throw new Error('No JSON response from JTAG command');
    }

    // Count braces to find complete JSON
    let braceCount = 0;
    let jsonEnd = jsonStart;
    for (let i = jsonStart; i < output.length; i++) {
      if (output[i] === '{') braceCount++;
      if (output[i] === '}') {
        braceCount--;
        if (braceCount === 0) {
          jsonEnd = i + 1;
          break;
        }
      }
    }

    return JSON.parse(output.substring(jsonStart, jsonEnd)) as T;
  } catch (error: unknown) {
    const err = error as { stdout?: string; stderr?: string; message?: string };
    console.error(`JTAG command failed: ${command}`);
    console.error(err.stderr || err.message);
    throw error;
  }
}

async function migrate(dryRun: boolean = false, batchSize: number = 100): Promise<MigrationStats> {
  const stats: MigrationStats = {
    totalRecords: 0,
    alreadyMigrated: 0,
    belowThreshold: 0,
    migrated: 0,
    errors: 0,
    totalSavedBytes: 0
  };

  // Initialize BlobStorage
  BlobStorage.initialize();

  // Check if JTAG server is running
  try {
    const pingResult = executeJtag<{ success: boolean }>('ping');
    if (!pingResult.success) {
      console.error('JTAG server is not responding. Run `npm start` first.');
      process.exit(1);
    }
  } catch {
    console.error('Cannot connect to JTAG server. Ensure `npm start` has completed.');
    process.exit(1);
  }

  // Get total count
  const countResult = executeJtag<{ success: boolean; items?: unknown[]; totalCount?: number }>(
    'data/list --collection=coordination_decisions --limit=1'
  );
  stats.totalRecords = countResult.totalCount || 0;
  console.log(`Total records: ${stats.totalRecords}`);

  if (stats.totalRecords === 0) {
    console.log('No records to migrate.');
    return stats;
  }

  // Process in batches using offset pagination
  let offset = 0;
  let processed = 0;

  while (offset < stats.totalRecords) {
    // Fetch batch - records with ragContext but no ragContextRef
    // Using filter to get unmigrated records
    const listResult = executeJtag<{ success: boolean; items?: CoordinationDecisionRecord[] }>(
      `data/list --collection=coordination_decisions --limit=${batchSize} --offset=${offset}`
    );

    const rows = listResult.items || [];
    if (rows.length === 0) {
      break;
    }

    for (const row of rows) {
      processed++;

      // Skip if already migrated
      if (row.ragContextRef && row.ragContextRef.startsWith('sha256:')) {
        stats.alreadyMigrated++;
        continue;
      }

      // Skip if no ragContext
      if (!row.ragContext) {
        continue;
      }

      // Parse ragContext if it's a string
      let ragContext: Record<string, unknown>;
      let ragContextStr: string;
      try {
        if (typeof row.ragContext === 'string') {
          ragContext = JSON.parse(row.ragContext);
          ragContextStr = row.ragContext;
        } else {
          ragContext = row.ragContext;
          ragContextStr = JSON.stringify(row.ragContext);
        }
      } catch (e) {
        console.error(`  [${row.id}] Failed to parse ragContext: ${e}`);
        stats.errors++;
        continue;
      }

      // Check size
      const sizeBytes = Buffer.byteLength(ragContextStr, 'utf8');

      if (sizeBytes < THRESHOLD_BYTES) {
        stats.belowThreshold++;
        continue;
      }

      // Store in BlobStorage
      if (dryRun) {
        console.log(`  [DRY-RUN] Would migrate ${row.id}: ${sizeBytes} bytes`);
        stats.migrated++;
        stats.totalSavedBytes += sizeBytes;
      } else {
        try {
          const blobRef = await BlobStorage.store(ragContext);

          // Update via Commands.execute (data/update)
          // Note: We set ragContext to null and ragContextRef to the hash
          const updateData = {
            ragContext: null,
            ragContextRef: blobRef.hash
          };

          const updateResult = executeJtag<{ success: boolean; error?: string }>(
            `data/update --collection=coordination_decisions --id=${row.id} --data='${JSON.stringify(updateData)}'`
          );

          if (!updateResult.success) {
            throw new Error(updateResult.error || 'Update failed');
          }

          const savedBytes = sizeBytes - blobRef.compressedSize;
          stats.migrated++;
          stats.totalSavedBytes += savedBytes;

          console.log(`  [${row.id}] Migrated: ${sizeBytes} → ${blobRef.compressedSize} bytes (saved ${savedBytes})`);
        } catch (e) {
          console.error(`  [${row.id}] Migration failed: ${e}`);
          stats.errors++;
        }
      }
    }

    offset += batchSize;
    console.log(`Progress: ${processed}/${stats.totalRecords} records processed`);
  }

  return stats;
}

async function main() {
  const args = process.argv.slice(2);
  const dryRun = args.includes('--dry-run');
  const batchSizeArg = args.find(a => a.startsWith('--batch-size='));
  const batchSize = batchSizeArg ? parseInt(batchSizeArg.split('=')[1], 10) : 100;

  console.log('='.repeat(60));
  console.log('RAG Context to Blob Storage Migration');
  console.log('='.repeat(60));
  console.log(`Mode: ${dryRun ? 'DRY RUN (no changes)' : 'LIVE (will modify database)'}`);
  console.log(`Batch size: ${batchSize}`);
  console.log(`Threshold: ${THRESHOLD_BYTES} bytes`);
  console.log('');
  console.log('NOTE: This script uses Commands.execute via JTAG CLI.');
  console.log('      Ensure JTAG server is running (`npm start` first).');
  console.log('');

  const startTime = Date.now();
  const stats = await migrate(dryRun, batchSize);
  const duration = Date.now() - startTime;

  console.log('');
  console.log('='.repeat(60));
  console.log('Migration Summary');
  console.log('='.repeat(60));
  console.log(`Total records:     ${stats.totalRecords}`);
  console.log(`Already migrated:  ${stats.alreadyMigrated}`);
  console.log(`Below threshold:   ${stats.belowThreshold}`);
  console.log(`Migrated:          ${stats.migrated}`);
  console.log(`Errors:            ${stats.errors}`);
  console.log(`Space saved:       ${(stats.totalSavedBytes / 1024 / 1024).toFixed(2)} MB`);
  console.log(`Duration:          ${(duration / 1000).toFixed(1)}s`);
  console.log('');

  if (dryRun && stats.migrated > 0) {
    console.log('Run without --dry-run to perform the actual migration.');
  }

  // After migration, recommend VACUUM
  if (!dryRun && stats.migrated > 0) {
    console.log('');
    console.log('RECOMMENDATION: Run database VACUUM to reclaim space:');
    console.log('  sqlite3 ~/.continuum/data/database.sqlite "VACUUM;"');
  }
}

main().catch(error => {
  console.error('Migration failed:', error);
  process.exit(1);
});
