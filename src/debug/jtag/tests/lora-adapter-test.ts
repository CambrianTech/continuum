/**
 * LoRA Adapter Integration Test
 *
 * Tests the full pipeline:
 * 1. Base model generation
 * 2. Load LoRA adapter
 * 3. Merged generation
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

async function main() {
  console.log('🧪 LoRA Adapter Integration Test\n');

  const client = new InferenceGrpcClient();

  try {
    // Step 1: Ping to verify connection
    console.log('1️⃣ Pinging server...');
    const ping = await client.ping();
    console.log(`   ✅ ${ping.message}\n`);

    // Step 2: Get status
    console.log('2️⃣ Checking status...');
    const status = await client.status();
    console.log(`   Model: ${status.currentModel}`);
    console.log(`   Healthy: ${status.healthy}`);
    console.log(`   Active adapters: ${status.activeAdapters.length}\n`);

    // Step 3: Generate with base model
    console.log('3️⃣ Generating with BASE model...');
    const prompt = 'Write a haiku about coding:';
    console.log(`   Prompt: "${prompt}"`);

    const baseResult = await client.generate(
      status.currentModel,
      prompt,
      { maxTokens: 50, temperature: 0.7, timeoutMs: 60000 }
    );
    console.log(`   Result: "${baseResult.text.trim()}"`);
    console.log(`   Tokens: ${baseResult.tokens}, Time: ${baseResult.durationMs}ms\n`);

    // Step 4: Load LoRA adapter
    console.log('4️⃣ Loading LoRA adapter...');
    const adapterPath = '/tmp/lora-test/jiten1024/adapter_model.safetensors';
    const loadResult = await client.loadAdapter(
      'jiten1024-test',
      adapterPath,
      { scale: 1.0, merge: true }
    );
    console.log(`   Success: ${loadResult.success}`);
    console.log(`   Load time: ${loadResult.loadTimeMs}ms`);
    if (loadResult.error) {
      console.log(`   Info: ${loadResult.error}`);
    }
    console.log('');

    // Step 5: List adapters
    console.log('5️⃣ Listing adapters...');
    const adapters = await client.listAdapters();
    for (const adapter of adapters) {
      console.log(`   - ${adapter.adapterId}: scale=${adapter.scale}, active=${adapter.active}`);
    }
    console.log('');

    // Step 6: Generate with merged LoRA
    console.log('6️⃣ Generating with MERGED LoRA...');
    const loraResult = await client.generate(
      status.currentModel,
      prompt,
      { maxTokens: 50, temperature: 0.7, timeoutMs: 60000 }
    );
    console.log(`   Result: "${loraResult.text.trim()}"`);
    console.log(`   Tokens: ${loraResult.tokens}, Time: ${loraResult.durationMs}ms\n`);

    // Step 7: Compare
    console.log('7️⃣ Comparison:');
    console.log(`   Base:  "${baseResult.text.trim().substring(0, 80)}..."`);
    console.log(`   LoRA:  "${loraResult.text.trim().substring(0, 80)}..."`);
    console.log('');

    // Step 8: Unload adapter
    console.log('8️⃣ Unloading adapter...');
    const unloadResult = await client.unloadAdapter('jiten1024-test');
    console.log(`   Success: ${unloadResult.success}\n`);

    console.log('✅ Test complete!');
  } catch (error) {
    console.error('❌ Error:', error);
    process.exit(1);
  } finally {
    client.close();
  }
}

main();
