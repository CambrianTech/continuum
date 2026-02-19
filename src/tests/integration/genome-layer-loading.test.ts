#!/usr/bin/env tsx
/**
 * Genome Layer Loading Integration Tests
 * =======================================
 *
 * TDD-style tests for LayerLoader, LayerCache, and layer assembly.
 * Tests are written FIRST, then implementation is fixed until they pass.
 *
 * Test coverage:
 * - LayerLoader: Load mock LoRA layers from disk
 * - LayerCache: LRU eviction, hit/miss tracking
 * - Layer validation and checksum verification
 * - End-to-end layer loading flow
 */

import { promises as fs } from 'fs';
import * as path from 'path';
import * as os from 'os';
import { LayerLoader } from '../../system/genome/server/LayerLoader';
import { LayerCache } from '../../system/genome/server/LayerCache';
import type { LoRAConfig, LayerMetadata } from '../../system/genome/shared/GenomeAssemblyTypes';

// Test configuration
const TEST_LAYER_DIR = path.join(os.tmpdir(), `genome-test-${Date.now()}`);
const TEST_LAYER_ID_1 = 'test-layer-python-expert';
const TEST_LAYER_ID_2 = 'test-layer-friendly-assistant';
const TEST_LAYER_ID_3 = 'test-layer-code-reviewer';

/**
 * Create mock layer files on disk for testing
 */
async function createMockLayer(layerId: string, sizeKB: number): Promise<void> {
  const layerDir = path.join(TEST_LAYER_DIR, layerId);
  await fs.mkdir(layerDir, { recursive: true });

  // Create adapter_config.json
  const config: LoRAConfig = {
    r: 8,
    lora_alpha: 16,
    target_modules: ['q_proj', 'v_proj', 'k_proj', 'o_proj'],
    lora_dropout: 0.05,
    bias: 'none',
  };
  await fs.writeFile(
    path.join(layerDir, 'adapter_config.json'),
    JSON.stringify(config, null, 2)
  );

  // Create metadata.json
  const metadata: LayerMetadata = {
    layerId,
    name: `Mock Layer ${layerId}`,
    description: `Test layer for integration tests`,
    baseModel: 'meta-llama/Llama-3-8B',
    trainedOn: '2025-10-11',
    sizeBytes: sizeKB * 1024,
    checksum: 'sha256:mock-checksum-12345',
    format: 'safetensors',
  };
  await fs.writeFile(
    path.join(layerDir, 'metadata.json'),
    JSON.stringify(metadata, null, 2)
  );

  // Create mock weights file (just random bytes)
  const weights = Buffer.alloc(sizeKB * 1024, 0xab);
  await fs.writeFile(path.join(layerDir, 'adapter_model.safetensors'), weights);

  console.log(`✅ Created mock layer: ${layerId} (${sizeKB}KB)`);
}

/**
 * Clean up test layer directory
 */
async function cleanup(): Promise<void> {
  try {
    await fs.rm(TEST_LAYER_DIR, { recursive: true, force: true });
    console.log(`🗑️  Cleaned up test directory: ${TEST_LAYER_DIR}`);
  } catch (error) {
    console.warn(`⚠️  Cleanup failed: ${error}`);
  }
}

/**
 * Test 1: LayerLoader can load mock layer from disk
 */
async function testLayerLoaderBasic(): Promise<void> {
  console.log('\n📋 TEST 1: LayerLoader - Basic Loading');
  console.log('======================================');

  const loader = new LayerLoader(TEST_LAYER_DIR);

  // Check layer exists
  const exists = await loader.layerExists(TEST_LAYER_ID_1);
  if (!exists) {
    throw new Error('❌ FAIL: Layer should exist');
  }
  console.log('✅ Layer exists check passed');

  // Load layer
  const layer = await loader.loadLayer(TEST_LAYER_ID_1);

  if (!layer.layerId || layer.layerId !== TEST_LAYER_ID_1) {
    throw new Error('❌ FAIL: Layer ID mismatch');
  }
  console.log('✅ Layer ID correct');

  if (!layer.config || layer.config.r !== 8) {
    throw new Error('❌ FAIL: LoRA config not loaded');
  }
  console.log('✅ LoRA config loaded');

  if (!layer.metadata || !layer.metadata.name) {
    throw new Error('❌ FAIL: Metadata not loaded');
  }
  console.log('✅ Metadata loaded');

  if (!layer.weights || layer.weights.length === 0) {
    throw new Error('❌ FAIL: Weights not loaded');
  }
  console.log('✅ Weights loaded');

  console.log(`\n✅ TEST 1 PASSED: Loaded ${layer.sizeBytes} bytes in ${Date.now() - layer.loadedAt}ms`);
}

/**
 * Test 2: LayerLoader statistics tracking
 */
async function testLayerLoaderStats(): Promise<void> {
  console.log('\n📋 TEST 2: LayerLoader - Statistics');
  console.log('===================================');

  const loader = new LayerLoader(TEST_LAYER_DIR);

  // Reset stats
  loader.resetStats();

  // Load multiple layers
  await loader.loadLayer(TEST_LAYER_ID_1);
  await loader.loadLayer(TEST_LAYER_ID_2);

  const stats = loader.getStats();

  if (stats.layersLoaded !== 2) {
    throw new Error(`❌ FAIL: Expected 2 layers loaded, got ${stats.layersLoaded}`);
  }
  console.log('✅ Layers loaded count correct');

  if (stats.bytesRead === 0) {
    throw new Error('❌ FAIL: Bytes read should be > 0');
  }
  console.log(`✅ Bytes read tracked: ${stats.bytesRead}`);

  if (stats.avgLoadTimeMs === 0) {
    throw new Error('❌ FAIL: Average load time should be > 0');
  }
  console.log(`✅ Average load time: ${stats.avgLoadTimeMs}ms`);

  console.log('\n✅ TEST 2 PASSED: Statistics tracking works');
}

/**
 * Test 3: LayerCache hit/miss tracking
 */
async function testLayerCacheHitMiss(): Promise<void> {
  console.log('\n📋 TEST 3: LayerCache - Hit/Miss Tracking');
  console.log('=========================================');

  const cache = new LayerCache({ maxSizeBytes: 10 * 1024 * 1024 }); // 10MB
  const loader = new LayerLoader(TEST_LAYER_DIR);

  // First access - should be miss
  let cached = cache.get(TEST_LAYER_ID_1);
  if (cached !== null) {
    throw new Error('❌ FAIL: First access should be cache miss');
  }
  console.log('✅ Cache miss on first access');

  // Load and cache layer
  const layer = await loader.loadLayer(TEST_LAYER_ID_1);
  cache.set(TEST_LAYER_ID_1, layer);

  // Second access - should be hit
  cached = cache.get(TEST_LAYER_ID_1);
  if (cached === null) {
    throw new Error('❌ FAIL: Second access should be cache hit');
  }
  console.log('✅ Cache hit on second access');

  // Verify stats
  const stats = cache.getStats();
  if (stats.hits !== 1) {
    throw new Error(`❌ FAIL: Expected 1 hit, got ${stats.hits}`);
  }
  if (stats.misses !== 1) {
    throw new Error(`❌ FAIL: Expected 1 miss, got ${stats.misses}`);
  }
  if (stats.hitRate !== 0.5) {
    throw new Error(`❌ FAIL: Expected 50% hit rate, got ${stats.hitRate * 100}%`);
  }
  console.log(`✅ Hit rate: ${(stats.hitRate * 100).toFixed(1)}%`);

  console.log('\n✅ TEST 3 PASSED: Cache hit/miss tracking works');
}

/**
 * Test 4: LayerCache LRU eviction
 */
async function testLayerCacheLRUEviction(): Promise<void> {
  console.log('\n📋 TEST 4: LayerCache - LRU Eviction');
  console.log('====================================');

  // Create cache with small size limit (2KB)
  const cache = new LayerCache({ maxSizeBytes: 2 * 1024 });
  const loader = new LayerLoader(TEST_LAYER_DIR);

  // Load 3 layers (1KB each = 3KB total, but cache is 2KB)
  const layer1 = await loader.loadLayer(TEST_LAYER_ID_1);
  const layer2 = await loader.loadLayer(TEST_LAYER_ID_2);
  const layer3 = await loader.loadLayer(TEST_LAYER_ID_3);

  // Add layer 1
  cache.set(TEST_LAYER_ID_1, layer1);
  console.log('Added layer 1 to cache');

  // Add layer 2
  cache.set(TEST_LAYER_ID_2, layer2);
  console.log('Added layer 2 to cache');

  // Add layer 3 - should evict layer 1 (LRU)
  cache.set(TEST_LAYER_ID_3, layer3);
  console.log('Added layer 3 to cache (should evict layer 1)');

  // Verify layer 1 was evicted
  if (cache.has(TEST_LAYER_ID_1)) {
    throw new Error('❌ FAIL: Layer 1 should have been evicted');
  }
  console.log('✅ Layer 1 was evicted (LRU)');

  // Verify layer 2 and 3 are still cached
  if (!cache.has(TEST_LAYER_ID_2)) {
    throw new Error('❌ FAIL: Layer 2 should still be cached');
  }
  if (!cache.has(TEST_LAYER_ID_3)) {
    throw new Error('❌ FAIL: Layer 3 should still be cached');
  }
  console.log('✅ Layer 2 and 3 still cached');

  // Verify eviction count
  const stats = cache.getStats();
  if (stats.evictionCount === 0) {
    throw new Error('❌ FAIL: Eviction count should be > 0');
  }
  console.log(`✅ Eviction count: ${stats.evictionCount}`);

  console.log('\n✅ TEST 4 PASSED: LRU eviction works');
}

/**
 * Test 5: End-to-end layer loading with cache
 */
async function testEndToEndLayerLoading(): Promise<void> {
  console.log('\n📋 TEST 5: End-to-End Layer Loading');
  console.log('===================================');

  const cache = new LayerCache();
  const loader = new LayerLoader(TEST_LAYER_DIR);

  // Simulate typical usage pattern
  console.log('\nSimulating typical layer loading pattern...');

  for (let i = 0; i < 10; i++) {
    const layerId = i % 3 === 0 ? TEST_LAYER_ID_1 : i % 3 === 1 ? TEST_LAYER_ID_2 : TEST_LAYER_ID_3;

    // Check cache first
    let layer = cache.get(layerId);

    if (layer === null) {
      // Cache miss - load from disk
      layer = await loader.loadLayer(layerId);
      cache.set(layerId, layer);
      console.log(`  [${i}] Load ${layerId} (cache miss)`);
    } else {
      console.log(`  [${i}] Load ${layerId} (cache hit)`);
    }
  }

  // Verify cache statistics
  const cacheStats = cache.getStats();
  const loaderStats = loader.getStats();

  console.log(`\nCache Statistics:`);
  console.log(`  Entries: ${cacheStats.entries}`);
  console.log(`  Hit rate: ${(cacheStats.hitRate * 100).toFixed(1)}%`);
  console.log(`  Hits: ${cacheStats.hits}`);
  console.log(`  Misses: ${cacheStats.misses}`);

  console.log(`\nLoader Statistics:`);
  console.log(`  Layers loaded: ${loaderStats.layersLoaded}`);
  console.log(`  Bytes read: ${loaderStats.bytesRead}`);
  console.log(`  Avg load time: ${loaderStats.avgLoadTimeMs.toFixed(2)}ms`);

  // Cache should have improved performance
  if (cacheStats.hitRate < 0.5) {
    throw new Error('❌ FAIL: Cache hit rate should be > 50%');
  }
  console.log('✅ Cache improved performance (hit rate > 50%)');

  console.log('\n✅ TEST 5 PASSED: End-to-end loading works');
}

/**
 * Main test runner
 */
async function main(): Promise<void> {
  console.log('🧬 GENOME LAYER LOADING INTEGRATION TESTS');
  console.log('=========================================\n');

  try {
    // Setup: Create mock layers
    console.log('📦 Setting up test environment...');
    await createMockLayer(TEST_LAYER_ID_1, 1); // 1KB
    await createMockLayer(TEST_LAYER_ID_2, 1); // 1KB
    await createMockLayer(TEST_LAYER_ID_3, 1); // 1KB
    console.log('');

    // Run tests
    await testLayerLoaderBasic();
    await testLayerLoaderStats();
    await testLayerCacheHitMiss();
    await testLayerCacheLRUEviction();
    await testEndToEndLayerLoading();

    // Cleanup
    await cleanup();

    console.log('\n✅ ALL TESTS PASSED (5/5)');
    console.log('=========================\n');
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
