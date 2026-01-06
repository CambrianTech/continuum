/**
 * HuggingFace Download Test (Milestone 3)
 *
 * Tests downloading adapter from HuggingFace Hub:
 * 1. Download adapter by repo ID
 * 2. Verify local storage in ~/.continuum/adapters/
 * 3. Apply genome with downloaded adapter
 * 4. Generate text to verify it works
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';
import * as fs from 'fs';
import * as path from 'path';

const TEST_REPO = 'Jiten1024/llama-3.2-3b-int-finetune-jav-rank-1-alpha-32';

async function main() {
  console.log('📥 HuggingFace Download Test (Milestone 3)\n');

  const client = new InferenceGrpcClient();
  const prompt = 'Write a haiku about downloading:';

  try {
    // Step 1: Verify connection
    console.log('1️⃣ Connecting...');
    const ping = await client.ping();
    console.log(`   ✅ ${ping.message}\n`);

    // Step 2: Download adapter from HuggingFace
    console.log(`2️⃣ Downloading adapter: ${TEST_REPO}...`);
    const download = await client.downloadAdapter(TEST_REPO, {
      adapterId: 'hf-jiten-test',
    });

    if (!download.success) {
      throw new Error(`Download failed: ${download.error}`);
    }

    console.log(`   ✅ Downloaded in ${download.downloadTimeMs}ms`);
    console.log(`   Adapter ID: ${download.adapterId}`);
    console.log(`   Local path: ${download.localPath}`);
    if (download.metadata) {
      console.log(`   Base model: ${download.metadata.baseModel}`);
      console.log(`   Rank: ${download.metadata.rank}, Alpha: ${download.metadata.alpha}`);
      console.log(`   Target modules: ${download.metadata.targetModules.join(', ')}`);
    }
    console.log('');

    // Step 3: Verify local storage
    console.log('3️⃣ Verifying local storage...');
    const localRegistryDir = path.join(
      process.env.HOME || '',
      '.continuum/adapters/installed',
      TEST_REPO.replace('/', '--')
    );
    const manifestPath = path.join(localRegistryDir, 'manifest.json');
    const weightsPath = path.join(localRegistryDir, 'adapter_model.safetensors');

    if (fs.existsSync(manifestPath)) {
      console.log(`   ✅ Manifest exists: ${manifestPath}`);
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
      console.log(`   Manifest: ${JSON.stringify(manifest, null, 2).substring(0, 200)}...`);
    } else {
      console.log(`   ⚠ Manifest not found at expected path`);
    }

    if (fs.existsSync(weightsPath)) {
      const stats = fs.statSync(weightsPath);
      console.log(`   ✅ Weights exist: ${(stats.size / 1024 / 1024).toFixed(2)} MB\n`);
    } else {
      console.log(`   ⚠ Weights not found at expected path\n`);
    }

    // Step 4: List adapters
    console.log('4️⃣ Listing loaded adapters...');
    const adapters = await client.listAdapters();
    for (const a of adapters) {
      console.log(`   - ${a.adapterId}: scale=${a.scale}, active=${a.active}`);
    }
    console.log('');

    // Step 5: Apply genome with downloaded adapter
    console.log('5️⃣ Applying genome with downloaded adapter...');
    const genome = await client.applyGenome([
      { adapterId: 'hf-jiten-test', scale: 1.0 },
    ]);
    console.log(`   Applied ${genome.adaptersApplied} adapter, ${genome.layersMerged} layers in ${genome.applyTimeMs}ms\n`);

    // Step 6: Generate text
    console.log('6️⃣ Generating with adapter...');
    const status = await client.status();
    const result = await client.generate(status.currentModel, prompt, {
      maxTokens: 50, temperature: 0.7, timeoutMs: 60000
    });
    console.log(`   Output: "${result.text.trim().substring(0, 150)}..."\n`);

    // Step 7: Clean up
    console.log('7️⃣ Cleaning up...');
    await client.unloadAdapter('hf-jiten-test');
    console.log('   ✅ Adapter unloaded\n');

    console.log('✅ HuggingFace download test complete!');
    console.log(`   Adapter stored in: ${localRegistryDir}`);

  } catch (error) {
    console.error('❌ Error:', error);
    process.exit(1);
  } finally {
    client.close();
  }
}

main();
