#!/usr/bin/env npx tsx
/**
 * Backfill Memory Embeddings Script
 *
 * Generates embeddings for memories that don't have them yet.
 * Run this to increase embedding coverage from ~5% to 80%+.
 *
 * Usage:
 *   npx tsx scripts/backfill-memory-embeddings.ts
 *   npx tsx scripts/backfill-memory-embeddings.ts --persona helper
 *   npx tsx scripts/backfill-memory-embeddings.ts --batch-size 10 --limit 1000
 */

import * as fs from 'fs';
import * as path from 'path';
import Database from 'better-sqlite3';

// Configuration
const CONFIG = {
  batchSize: parseInt(process.env.BATCH_SIZE || '5', 10),  // Embeddings per batch (avoid overwhelming Ollama)
  limit: parseInt(process.env.LIMIT || '5000', 10),        // Max memories to process
  delayBetweenBatches: 1000,                                // 1 second between batches
  timeoutMs: 60000,                                         // 60 second timeout per embedding
  personaFilter: process.argv.find(a => a.startsWith('--persona='))?.split('=')[1] || null,
};

// Parse command line args
for (let i = 2; i < process.argv.length; i++) {
  const arg = process.argv[i];
  if (arg === '--batch-size' && process.argv[i + 1]) {
    CONFIG.batchSize = parseInt(process.argv[++i], 10);
  } else if (arg === '--limit' && process.argv[i + 1]) {
    CONFIG.limit = parseInt(process.argv[++i], 10);
  } else if (arg === '--persona' && process.argv[i + 1]) {
    CONFIG.personaFilter = process.argv[++i];
  }
}

// Ollama embedding configuration
const OLLAMA_URL = process.env.OLLAMA_URL || 'http://localhost:11434';
const EMBEDDING_MODEL = {
  name: 'all-minilm',
  dimensions: 384
};

interface MemoryRow {
  id: string;
  content: string;
  embedding: string | null;
}

async function findPersonaDatabases(): Promise<Array<{ persona: string; dbPath: string }>> {
  const baseDir = path.join(process.cwd(), '.continuum', 'personas');
  const databases: Array<{ persona: string; dbPath: string }> = [];

  if (!fs.existsSync(baseDir)) {
    console.error(`No personas directory found at ${baseDir}`);
    return [];
  }

  const personas = fs.readdirSync(baseDir);
  for (const persona of personas) {
    if (CONFIG.personaFilter && persona !== CONFIG.personaFilter) {
      continue;
    }

    const dbPath = path.join(baseDir, persona, 'data', 'longterm.db');
    if (fs.existsSync(dbPath)) {
      databases.push({ persona, dbPath });
    }
  }

  return databases;
}

async function getMemoriesWithoutEmbeddings(db: Database.Database, limit: number): Promise<MemoryRow[]> {
  // Find memories where embedding is NULL or empty
  const query = `
    SELECT id, content, embedding
    FROM memories
    WHERE embedding IS NULL OR embedding = '' OR length(embedding) < 10
    LIMIT ?
  `;
  return db.prepare(query).all(limit) as MemoryRow[];
}

async function generateEmbedding(text: string): Promise<number[] | null> {
  try {
    const response = await fetch(`${OLLAMA_URL}/api/embeddings`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        model: EMBEDDING_MODEL.name,
        prompt: text.slice(0, 8000)  // Limit text length
      })
    });

    if (!response.ok) {
      throw new Error(`Ollama returned ${response.status}: ${response.statusText}`);
    }

    const data = await response.json() as { embedding?: number[] };
    return data.embedding || null;
  } catch (error) {
    console.error(`  Embedding failed: ${error}`);
    return null;
  }
}

async function updateMemoryEmbedding(
  db: Database.Database,
  memoryId: string,
  embedding: number[]
): Promise<boolean> {
  try {
    const embeddingJson = JSON.stringify(embedding);

    // Only update the embedding column (schema may not have embeddedAt/embeddingModel)
    db.prepare(`
      UPDATE memories
      SET embedding = ?
      WHERE id = ?
    `).run(embeddingJson, memoryId);

    return true;
  } catch (error) {
    console.error(`  Failed to update memory ${memoryId}: ${error}`);
    return false;
  }
}

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

async function backfillPersona(persona: string, dbPath: string): Promise<{ success: number; failed: number; skipped: number }> {
  console.log(`\n${'='.repeat(60)}`);
  console.log(`Processing: ${persona}`);
  console.log(`Database: ${dbPath}`);
  console.log(`${'='.repeat(60)}`);

  const db = new Database(dbPath);
  const stats = { success: 0, failed: 0, skipped: 0 };

  try {
    // Get total count
    const totalResult = db.prepare('SELECT COUNT(*) as count FROM memories').get() as { count: number };
    const withEmbeddingResult = db.prepare(
      "SELECT COUNT(*) as count FROM memories WHERE embedding IS NOT NULL AND length(embedding) > 10"
    ).get() as { count: number };

    console.log(`Total memories: ${totalResult.count}`);
    console.log(`With embeddings: ${withEmbeddingResult.count} (${((withEmbeddingResult.count / totalResult.count) * 100).toFixed(1)}%)`);
    console.log(`Without embeddings: ${totalResult.count - withEmbeddingResult.count}`);
    console.log(`Target: Process up to ${CONFIG.limit} memories in batches of ${CONFIG.batchSize}`);
    console.log();

    // Get memories without embeddings
    const memories = await getMemoriesWithoutEmbeddings(db, CONFIG.limit);
    console.log(`Found ${memories.length} memories to process\n`);

    if (memories.length === 0) {
      console.log('No memories need embeddings!');
      return stats;
    }

    // Process in batches
    let batchNum = 0;
    for (let i = 0; i < memories.length; i += CONFIG.batchSize) {
      batchNum++;
      const batch = memories.slice(i, i + CONFIG.batchSize);
      console.log(`Batch ${batchNum}: Processing ${batch.length} memories (${i + 1}-${i + batch.length}/${memories.length})`);

      for (const memory of batch) {
        // Skip if content is empty
        if (!memory.content || memory.content.trim().length === 0) {
          console.log(`  [${memory.id.slice(0, 8)}] Skipped: empty content`);
          stats.skipped++;
          continue;
        }

        // Generate embedding
        const startTime = Date.now();
        const embedding = await generateEmbedding(memory.content);
        const elapsed = Date.now() - startTime;

        if (embedding) {
          const updated = await updateMemoryEmbedding(db, memory.id, embedding);
          if (updated) {
            console.log(`  [${memory.id.slice(0, 8)}] OK: ${embedding.length} dims in ${elapsed}ms`);
            stats.success++;
          } else {
            stats.failed++;
          }
        } else {
          console.log(`  [${memory.id.slice(0, 8)}] FAILED after ${elapsed}ms`);
          stats.failed++;
        }
      }

      // Delay between batches to avoid overwhelming Ollama
      if (i + CONFIG.batchSize < memories.length) {
        console.log(`  Waiting ${CONFIG.delayBetweenBatches}ms before next batch...\n`);
        await sleep(CONFIG.delayBetweenBatches);
      }
    }

    // Final stats
    const finalWithEmbedding = db.prepare(
      "SELECT COUNT(*) as count FROM memories WHERE embedding IS NOT NULL AND length(embedding) > 10"
    ).get() as { count: number };

    console.log(`\n--- ${persona} Summary ---`);
    console.log(`Success: ${stats.success}`);
    console.log(`Failed: ${stats.failed}`);
    console.log(`Skipped: ${stats.skipped}`);
    console.log(`New coverage: ${finalWithEmbedding.count}/${totalResult.count} (${((finalWithEmbedding.count / totalResult.count) * 100).toFixed(1)}%)`);

  } finally {
    db.close();
  }

  return stats;
}

async function main() {
  console.log('Memory Embedding Backfill Script');
  console.log('================================');
  console.log(`Config: batchSize=${CONFIG.batchSize}, limit=${CONFIG.limit}`);
  if (CONFIG.personaFilter) {
    console.log(`Filtering: only ${CONFIG.personaFilter}`);
  }

  // Check Ollama is available
  console.log(`\nChecking Ollama at ${OLLAMA_URL}...`);
  try {
    const health = await fetch(`${OLLAMA_URL}/api/tags`);
    if (!health.ok) throw new Error(`Status ${health.status}`);
    console.log('Ollama is ready');
  } catch (error) {
    console.error(`Cannot connect to Ollama: ${error}`);
    console.error('Make sure Ollama is running: ollama serve');
    process.exit(1);
  }

  const databases = await findPersonaDatabases();

  if (databases.length === 0) {
    console.error('No persona databases found!');
    process.exit(1);
  }

  console.log(`Found ${databases.length} persona database(s)`);

  const totals = { success: 0, failed: 0, skipped: 0 };

  for (const { persona, dbPath } of databases) {
    const stats = await backfillPersona(persona, dbPath);
    totals.success += stats.success;
    totals.failed += stats.failed;
    totals.skipped += stats.skipped;
  }

  console.log('\n' + '='.repeat(60));
  console.log('OVERALL TOTALS');
  console.log('='.repeat(60));
  console.log(`Success: ${totals.success}`);
  console.log(`Failed: ${totals.failed}`);
  console.log(`Skipped: ${totals.skipped}`);
  console.log(`Total processed: ${totals.success + totals.failed + totals.skipped}`);
}

main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
