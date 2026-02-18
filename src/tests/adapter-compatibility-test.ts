/**
 * Adapter Compatibility Test
 *
 * Tests each installed adapter to diagnose which ones work with our base model.
 * Outputs diagnostic information about why adapters fail.
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';
import { existsSync, readdirSync, readFileSync } from 'fs';
import { resolve } from 'path';

const ADAPTERS_DIR = '/Users/joel/.continuum/adapters/installed';
const TEST_PROMPT = 'Hello, how are you today?';

interface AdapterManifest {
  alpha: number;
  base_model: string;
  peft_type: string;
  rank: number;
  repo_id: string;
  target_modules: string[];
}

async function testAdapter(client: InferenceGrpcClient, name: string, path: string, manifest: AdapterManifest): Promise<void> {
  console.log(`\n${'='.repeat(60)}`);
  console.log(`Testing: ${name}`);
  console.log(`${'='.repeat(60)}`);
  console.log(`  Base model: ${manifest.base_model}`);
  console.log(`  Rank: ${manifest.rank}, Alpha: ${manifest.alpha}`);
  console.log(`  Proper scale (α/r): ${manifest.alpha / manifest.rank}`);
  console.log(`  Target modules: ${manifest.target_modules.join(', ')}`);

  // Check for compatibility issues
  const issues: string[] = [];

  // Issue 1: Base model mismatch
  if (!manifest.base_model.includes('unsloth/Llama-3.2-3B-Instruct')) {
    if (manifest.base_model.includes('4bit') || manifest.base_model.includes('bnb')) {
      issues.push(`❌ QUANTIZED: Adapter trained on 4-bit model, we use BF16`);
    } else if (manifest.base_model.includes('meta-llama')) {
      issues.push(`⚠️  BASE MISMATCH: Trained on meta-llama, we use unsloth`);
    } else {
      issues.push(`❌ WRONG MODEL: ${manifest.base_model}`);
    }
  }

  // Issue 2: Extreme alpha/rank ratios
  const scale = manifest.alpha / manifest.rank;
  if (scale > 10) {
    issues.push(`⚠️  HIGH SCALE: ${scale} (may amplify errors)`);
  } else if (scale < 0.1) {
    issues.push(`⚠️  LOW SCALE: ${scale} (may have no effect)`);
  }

  if (issues.length > 0) {
    console.log(`\n  Potential issues:`);
    issues.forEach(issue => console.log(`    ${issue}`));
  }

  // Test with proper scale (alpha/rank)
  const properScale = manifest.alpha / manifest.rank;

  console.log(`\n  Loading adapter with scale=${properScale.toFixed(2)}...`);

  try {
    const loadResult = await client.loadAdapter(name, path, {
      scale: properScale,
      merge: true,
    });

    if (!loadResult.success) {
      console.log(`  ❌ LOAD FAILED: ${loadResult.error}`);
      return;
    }

    console.log(`  ✅ Loaded in ${loadResult.loadTimeMs}ms, merged ${loadResult.layersMerged} layers`);

    // Test generation
    console.log(`\n  Testing generation...`);
    const result = await client.generate('Llama-3.2-3B-Instruct', TEST_PROMPT, {
      maxTokens: 30,
      temperature: 0.7,
      timeoutMs: 30000,
    });

    // Check for garbage output patterns
    const text = result.text;
    const hasGarbage = /\[\/[^\]]*\]/.test(text) || // [/xxx] pattern
                       /[\u0600-\u06FF]/.test(text) || // Arabic characters out of context
                       text.includes('[/') ||
                       (text.match(/\[/g)?.length || 0) > 5; // Too many brackets

    if (hasGarbage) {
      console.log(`  ❌ GARBAGE OUTPUT detected:`);
      console.log(`     "${text.slice(0, 100)}..."`);
    } else {
      console.log(`  ✅ GOOD OUTPUT:`);
      console.log(`     "${text.slice(0, 100)}..."`);
    }

  } catch (err) {
    console.log(`  ❌ ERROR: ${err instanceof Error ? err.message : err}`);
  }
}

async function main() {
  console.log('Adapter Compatibility Test');
  console.log('==========================\n');

  const client = InferenceGrpcClient.sharedInstance();

  // Test connection
  try {
    const pong = await client.ping();
    console.log(`✅ Connected to inference server: ${pong.message}`);
  } catch (err) {
    console.error(`❌ Cannot connect to inference server. Start it first!`);
    process.exit(1);
  }

  // First, test base model without any adapter
  console.log('\n--- BASELINE TEST (no adapter) ---');
  try {
    const baseline = await client.generate('Llama-3.2-3B-Instruct', TEST_PROMPT, {
      maxTokens: 30,
      temperature: 0.7,
      timeoutMs: 30000,
    });
    console.log(`Base model output: "${baseline.text.slice(0, 100)}..."`);

    const hasGarbage = /\[\/[^\]]*\]/.test(baseline.text) || baseline.text.includes('[/');
    if (hasGarbage) {
      console.log('⚠️  Base model already producing garbage - worker needs restart!');
      process.exit(1);
    }
  } catch (err) {
    console.error(`❌ Base model test failed: ${err}`);
    process.exit(1);
  }

  // List adapters
  if (!existsSync(ADAPTERS_DIR)) {
    console.error(`No adapters directory at ${ADAPTERS_DIR}`);
    process.exit(1);
  }

  const adapterDirs = readdirSync(ADAPTERS_DIR, { withFileTypes: true })
    .filter(d => d.isDirectory() && !d.name.startsWith('.'))
    .map(d => d.name);

  console.log(`\nFound ${adapterDirs.length} adapters to test\n`);

  // Test each adapter (need fresh model for each!)
  for (const adapterDir of adapterDirs) {
    const adapterPath = resolve(ADAPTERS_DIR, adapterDir);
    const manifestPath = resolve(adapterPath, 'manifest.json');
    const weightsPath = resolve(adapterPath, 'adapter_model.safetensors');

    if (!existsSync(manifestPath)) {
      console.log(`\n⚠️  Skipping ${adapterDir}: No manifest.json`);
      continue;
    }

    if (!existsSync(weightsPath)) {
      console.log(`\n⚠️  Skipping ${adapterDir}: No adapter_model.safetensors`);
      continue;
    }

    try {
      const manifest: AdapterManifest = JSON.parse(readFileSync(manifestPath, 'utf-8'));
      await testAdapter(client, adapterDir, weightsPath, manifest);

      // IMPORTANT: After merge, model is permanently modified!
      // We need to reload the base model for next test
      console.log(`\n  ⚠️  Model weights modified - need to restart worker for next test`);
      console.log(`     (Merge is permanent in current implementation)`);
      break; // Can only test one adapter per run

    } catch (err) {
      console.log(`\n❌ Error testing ${adapterDir}: ${err}`);
    }
  }

  console.log('\n\nTest complete.');
  console.log('Note: To test another adapter, restart the inference worker to reset model weights.');

  client.close();
}

main().catch(console.error);
