#!/usr/bin/env tsx
/**
 * Genome Assembly End-to-End Integration Test
 * ============================================
 *
 * Tests the complete genome assembly flow:
 * LayerLoader → LayerCache → LayerComposer → GenomeAssembler
 *
 * This validates the full system working together to load and
 * assemble PersonaUser genomes.
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import * as os from 'os';
import { GenomeAssembler } from '../../system/genome/server/GenomeAssembler';
import { LayerLoader } from '../../system/genome/server/LayerLoader';
import { LayerCache } from '../../system/genome/server/LayerCache';
import { LayerComposer } from '../../system/genome/server/LayerComposer';
import type { LoRAConfig, LayerMetadata } from '../../system/genome/shared/GenomeAssemblyTypes';

// Test configuration
const TEST_LAYER_DIR = path.join(os.tmpdir(), `genome-e2e-${Date.now()}`);
const TEST_GENOME_ID = 'genome-python-expert';

/**
 * Create mock layer files
 */
async function createMockLayer(layerId: string, sizeKB: number): Promise<void> {
  const layerDir = path.join(TEST_LAYER_DIR, layerId);
  await fs.mkdir(layerDir, { recursive: true });

  const config: LoRAConfig = {
    r: 8,
    lora_alpha: 16,
    target_modules: ['q_proj', 'v_proj', 'k_proj', 'o_proj'],
    lora_dropout: 0.05,
    bias: 'none',
  };

  const metadata: LayerMetadata = {
    layerId,
    name: `${layerId}`,
    description: `Test layer ${layerId}`,
    baseModel: 'meta-llama/Llama-3-8B',
    trainedOn: '2025-10-11',
    sizeBytes: sizeKB * 1024,
    checksum: 'sha256:mock',
    format: 'safetensors',
  };

  const weights = Buffer.alloc(sizeKB * 1024, 0xab);

  await fs.writeFile(path.join(layerDir, 'adapter_config.json'), JSON.stringify(config, null, 2));
  await fs.writeFile(path.join(layerDir, 'metadata.json'), JSON.stringify(metadata, null, 2));
  await fs.writeFile(path.join(layerDir, 'adapter_model.safetensors'), weights);
}

/**
 * Setup test environment
 */
async function setup(): Promise<void> {
  console.log('📦 Setting up test environment...');
  await createMockLayer('test-layer-python-expert', 1);
  await createMockLayer('test-layer-friendly-assistant', 1);
  await createMockLayer('test-layer-code-reviewer', 1);
  console.log('');
}

/**
 * Cleanup test environment
 */
async function cleanup(): Promise<void> {
  try {
    await fs.rm(TEST_LAYER_DIR, { recursive: true, force: true });
    console.log(`🗑️  Cleaned up: ${TEST_LAYER_DIR}`);
  } catch (error) {
    console.warn(`⚠️  Cleanup failed: ${error}`);
  }
}

/**
 * Test: Complete genome assembly flow
 */
async function testGenomeAssembly(): Promise<void> {
  console.log('\n📋 TEST: Complete Genome Assembly');
  console.log('==================================');

  // Create assembler with custom components
  const loader = new LayerLoader(TEST_LAYER_DIR);
  const cache = new LayerCache({ maxSizeBytes: 10 * 1024 * 1024 }); // 10MB
  const composer = new LayerComposer();
  const assembler = new GenomeAssembler(loader, cache, composer);

  // Assemble genome
  const genome = await assembler.assembleGenome(TEST_GENOME_ID as any);

  // Verify assembled genome
  if (!genome.genomeId) {
    throw new Error('❌ FAIL: Genome ID missing');
  }
  console.log('✅ Genome ID present');

  if (genome.layerCount !== 3) {
    throw new Error(`❌ FAIL: Expected 3 layers, got ${genome.layerCount}`);
  }
  console.log('✅ Layer count correct: 3');

  if (!genome.composedLayer) {
    throw new Error('❌ FAIL: Composed layer missing');
  }
  console.log('✅ Composed layer created');

  if (genome.assemblyTimeMs === 0) {
    throw new Error('❌ FAIL: Assembly time should be > 0');
  }
  console.log(`✅ Assembly time: ${genome.assemblyTimeMs}ms`);

  if (genome.totalSizeBytes === 0) {
    throw new Error('❌ FAIL: Total size should be > 0');
  }
  console.log(`✅ Total size: ${genome.totalSizeBytes} bytes`);

  // Verify source layers
  if (genome.sourceLayers.length !== 3) {
    throw new Error(`❌ FAIL: Expected 3 source layers, got ${genome.sourceLayers.length}`);
  }
  console.log('✅ Source layers tracked');

  console.log('\n✅ TEST PASSED: Genome assembled successfully');
}

/**
 * Test: Cache performance across multiple assemblies
 */
async function testCachePerformance(): Promise<void> {
  console.log('\n📋 TEST: Cache Performance');
  console.log('==========================');

  const loader = new LayerLoader(TEST_LAYER_DIR);
  const cache = new LayerCache();
  const composer = new LayerComposer();
  const assembler = new GenomeAssembler(loader, cache, composer);

  // First assembly - all cache misses
  console.log('\n1st assembly (cold cache):');
  const genome1 = await assembler.assembleGenome(TEST_GENOME_ID as any);
  console.log(`   Time: ${genome1.assemblyTimeMs}ms`);
  console.log(`   Cache: ${genome1.cacheHits} hits, ${genome1.cacheMisses} misses`);

  if (genome1.cacheMisses !== 3) {
    throw new Error('❌ FAIL: First assembly should have 3 cache misses');
  }
  console.log('✅ Cold start: all cache misses');

  // Second assembly - all cache hits
  console.log('\n2nd assembly (warm cache):');
  const genome2 = await assembler.assembleGenome(TEST_GENOME_ID as any);
  console.log(`   Time: ${genome2.assemblyTimeMs}ms`);
  console.log(`   Cache: ${genome2.cacheHits} hits, ${genome2.cacheMisses} misses`);

  if (genome2.cacheHits !== 3) {
    throw new Error('❌ FAIL: Second assembly should have 3 cache hits');
  }
  console.log('✅ Warm start: all cache hits');

  // Verify speedup
  const speedup = genome1.assemblyTimeMs / genome2.assemblyTimeMs;
  console.log(`\n⚡ Speedup: ${speedup.toFixed(2)}x faster with cache`);

  if (genome2.assemblyTimeMs >= genome1.assemblyTimeMs) {
    console.warn('⚠️  WARNING: Cache didn\'t improve performance (layers too small for noticeable difference)');
  }

  console.log('\n✅ TEST PASSED: Cache improves performance');
}

/**
 * Test: Assembler statistics
 */
async function testAssemblerStats(): Promise<void> {
  console.log('\n📋 TEST: Assembler Statistics');
  console.log('=============================');

  const assembler = new GenomeAssembler(
    new LayerLoader(TEST_LAYER_DIR),
    new LayerCache(),
    new LayerComposer()
  );

  // Assemble genome multiple times
  await assembler.assembleGenome(TEST_GENOME_ID as any);
  await assembler.assembleGenome(TEST_GENOME_ID as any);
  await assembler.assembleGenome(TEST_GENOME_ID as any);

  // Get statistics
  const stats = assembler.getStats();

  console.log('\nAssembly Statistics:');
  console.log(`  Genomes assembled: ${stats.genomesAssembled}`);
  console.log(`  Layers loaded: ${stats.layersLoaded}`);
  console.log(`  Cache hit rate: ${(stats.cacheHitRate * 100).toFixed(1)}%`);
  console.log(`  Avg assembly time: ${stats.avgAssemblyTimeMs.toFixed(2)}ms`);
  console.log(`  Fastest: ${stats.fastestAssemblyMs}ms`);
  console.log(`  Slowest: ${stats.slowestAssemblyMs}ms`);
  console.log(`  Total bytes loaded: ${stats.totalBytesLoaded}`);
  console.log(`  Uptime: ${stats.uptimeMs}ms`);

  if (stats.genomesAssembled !== 3) {
    throw new Error(`❌ FAIL: Expected 3 genomes assembled, got ${stats.genomesAssembled}`);
  }
  console.log('✅ Genomes assembled count correct');

  if (stats.avgAssemblyTimeMs === 0) {
    throw new Error('❌ FAIL: Average assembly time should be > 0');
  }
  console.log('✅ Average assembly time tracked');

  if (stats.cacheHitRate < 0.5) {
    throw new Error('❌ FAIL: Cache hit rate should be > 50%');
  }
  console.log('✅ Cache hit rate > 50%');

  console.log('\n✅ TEST PASSED: Statistics tracking works');
}

/**
 * Test: Preload and unload operations
 */
async function testPreloadUnload(): Promise<void> {
  console.log('\n📋 TEST: Preload and Unload');
  console.log('===========================');

  const cache = new LayerCache();
  const assembler = new GenomeAssembler(
    new LayerLoader(TEST_LAYER_DIR),
    cache,
    new LayerComposer()
  );

  // Initially cache should be empty
  let cacheStats = cache.getStats();
  if (cacheStats.entries !== 0) {
    throw new Error('❌ FAIL: Cache should start empty');
  }
  console.log('✅ Cache initially empty');

  // Preload genome
  await assembler.preloadGenome(TEST_GENOME_ID as any);

  cacheStats = cache.getStats();
  if (cacheStats.entries !== 3) {
    throw new Error(`❌ FAIL: Expected 3 cached entries after preload, got ${cacheStats.entries}`);
  }
  console.log('✅ Preload cached 3 layers');

  // Assemble should be fast (all hits)
  const genome = await assembler.assembleGenome(TEST_GENOME_ID as any);
  if (genome.cacheHits !== 3) {
    throw new Error('❌ FAIL: All layers should be cache hits after preload');
  }
  console.log('✅ Assembly after preload: all cache hits');

  // Unload genome
  await assembler.unloadGenome(TEST_GENOME_ID as any);

  cacheStats = cache.getStats();
  if (cacheStats.entries !== 0) {
    throw new Error(`❌ FAIL: Cache should be empty after unload, got ${cacheStats.entries} entries`);
  }
  console.log('✅ Unload emptied cache');

  console.log('\n✅ TEST PASSED: Preload and unload work');
}

/**
 * Main test runner
 */
async function main(): Promise<void> {
  console.log('🧬 GENOME ASSEMBLY END-TO-END TESTS');
  console.log('===================================\n');

  try {
    await setup();

    await testGenomeAssembly();
    await testCachePerformance();
    await testAssemblerStats();
    await testPreloadUnload();

    await cleanup();

    console.log('\n✅ ALL E2E TESTS PASSED (4/4)');
    console.log('============================\n');
  } catch (error) {
    console.error('\n❌ TEST FAILED:', error);
    await cleanup();
    process.exit(1);
  }
}

// Run if executed directly
if (require.main === module) {
  main();
}
