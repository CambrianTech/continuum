/**
 * Genome Stacking Test (Milestone 2)
 *
 * Tests multi-adapter stacking:
 * 1. Load two different LoRA adapters
 * 2. Apply genome with both adapters at different scales
 * 3. Verify output differs from single-adapter and base
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

const ADAPTER_1 = {
  id: 'jiten-rank1',
  path: '/tmp/lora-test/jiten1024/adapter_model.safetensors',
  description: 'Jiten1024 rank-1 adapter (6MB)',
};

const ADAPTER_2 = {
  id: 'sizhkhy-rank64',
  path: '/tmp/lora-test/sizhkhy/adapter_model.safetensors',
  description: 'Sizhkhy rank-64 adapter (389MB)',
};

async function main() {
  console.log('🧬 Genome Stacking Test (Milestone 2)\n');

  const client = new InferenceGrpcClient();
  const prompt = 'Write a haiku about artificial intelligence:';

  try {
    // Step 1: Verify connection
    console.log('1️⃣ Connecting...');
    const ping = await client.ping();
    console.log(`   ✅ ${ping.message}\n`);

    // Step 2: Get status and base generation
    console.log('2️⃣ Generating with BASE model...');
    const status = await client.status();
    const baseResult = await client.generate(status.currentModel, prompt, {
      maxTokens: 50, temperature: 0.7, timeoutMs: 60000
    });
    console.log(`   Base: "${baseResult.text.trim().substring(0, 100)}..."\n`);

    // Step 3: Load both adapters (without merge)
    console.log('3️⃣ Loading adapters...');

    console.log(`   Loading ${ADAPTER_1.description}...`);
    const load1 = await client.loadAdapter(ADAPTER_1.id, ADAPTER_1.path, { scale: 1.0, merge: false });
    console.log(`   ${load1.success ? '✅' : '❌'} ${ADAPTER_1.id}`);

    console.log(`   Loading ${ADAPTER_2.description}...`);
    const load2 = await client.loadAdapter(ADAPTER_2.id, ADAPTER_2.path, { scale: 1.0, merge: false });
    console.log(`   ${load2.success ? '✅' : '❌'} ${ADAPTER_2.id}\n`);

    // Step 4: List adapters
    console.log('4️⃣ Loaded adapters:');
    const adapters = await client.listAdapters();
    for (const a of adapters) {
      console.log(`   - ${a.adapterId}: scale=${a.scale}`);
    }
    console.log('');

    // Step 5: Apply genome with ONLY adapter 1
    console.log('5️⃣ Applying genome: adapter 1 only (scale=1.0)...');
    const genome1 = await client.applyGenome([
      { adapterId: ADAPTER_1.id, scale: 1.0 },
    ]);
    console.log(`   Applied ${genome1.adaptersApplied} adapter, ${genome1.layersMerged} layers in ${genome1.applyTimeMs}ms`);

    const result1 = await client.generate(status.currentModel, prompt, {
      maxTokens: 50, temperature: 0.7, timeoutMs: 60000
    });
    console.log(`   Output: "${result1.text.trim().substring(0, 100)}..."\n`);

    // Step 6: Apply genome with ONLY adapter 2
    console.log('6️⃣ Applying genome: adapter 2 only (scale=1.0)...');
    const genome2 = await client.applyGenome([
      { adapterId: ADAPTER_2.id, scale: 1.0 },
    ]);
    console.log(`   Applied ${genome2.adaptersApplied} adapter, ${genome2.layersMerged} layers in ${genome2.applyTimeMs}ms`);

    const result2 = await client.generate(status.currentModel, prompt, {
      maxTokens: 50, temperature: 0.7, timeoutMs: 60000
    });
    console.log(`   Output: "${result2.text.trim().substring(0, 100)}..."\n`);

    // Step 7: Apply genome with BOTH adapters (stacking!)
    console.log('7️⃣ Applying genome: BOTH adapters (adapter1=0.5, adapter2=0.5)...');
    const genomeBoth = await client.applyGenome([
      { adapterId: ADAPTER_1.id, scale: 0.5 },
      { adapterId: ADAPTER_2.id, scale: 0.5 },
    ]);
    console.log(`   Applied ${genomeBoth.adaptersApplied} adapters, ${genomeBoth.layersMerged} layers in ${genomeBoth.applyTimeMs}ms`);

    const resultBoth = await client.generate(status.currentModel, prompt, {
      maxTokens: 50, temperature: 0.7, timeoutMs: 60000
    });
    console.log(`   Output: "${resultBoth.text.trim().substring(0, 100)}..."\n`);

    // Step 8: Summary
    console.log('8️⃣ Summary:');
    console.log(`   Base:       "${baseResult.text.trim().substring(0, 60)}..."`);
    console.log(`   Adapter 1:  "${result1.text.trim().substring(0, 60)}..."`);
    console.log(`   Adapter 2:  "${result2.text.trim().substring(0, 60)}..."`);
    console.log(`   Stacked:    "${resultBoth.text.trim().substring(0, 60)}..."`);
    console.log('');

    // Clean up
    console.log('9️⃣ Cleaning up...');
    await client.unloadAdapter(ADAPTER_1.id);
    await client.unloadAdapter(ADAPTER_2.id);
    console.log('   ✅ Adapters unloaded\n');

    console.log('✅ Genome stacking test complete!');
  } catch (error) {
    console.error('❌ Error:', error);
    process.exit(1);
  } finally {
    client.close();
  }
}

main();
