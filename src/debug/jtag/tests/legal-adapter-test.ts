/**
 * Test the Legal adapter specifically with proper scale (0.5)
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

const LEGAL_ADAPTER_PATH = '/Users/joel/.continuum/adapters/installed/sartajbhuvaji--Legal-Llama-3.2-3B-Instruct/adapter_model.safetensors';
const TEST_PROMPT = 'Hello, how are you today?';

async function main() {
  const client = InferenceGrpcClient.sharedInstance();

  // Test baseline
  console.log('=== BASELINE TEST (no adapter) ===');
  const baseline = await client.generate('Llama-3.2-3B-Instruct', TEST_PROMPT, {
    maxTokens: 30,
    temperature: 0.7,
    timeoutMs: 30000,
  });
  console.log(`Baseline: "${baseline.text.slice(0, 100)}..."\n`);

  // Test with proper scale (alpha/rank = 16/32 = 0.5)
  console.log('=== LEGAL ADAPTER TEST (scale=0.5) ===');
  console.log('Loading adapter...');

  const loadResult = await client.loadAdapter('legal-test', LEGAL_ADAPTER_PATH, {
    scale: 0.5,  // Proper scale: alpha/rank = 16/32
    merge: true,
  });

  console.log(`Load result: success=${loadResult.success}, time=${loadResult.loadTimeMs}ms`);

  if (loadResult.success) {
    console.log('Testing generation...');
    const result = await client.generate('Llama-3.2-3B-Instruct', TEST_PROMPT, {
      maxTokens: 30,
      temperature: 0.7,
      timeoutMs: 30000,
    });

    console.log(`Output: "${result.text}"`);

    // Check for garbage
    const hasGarbage = /\[\/[^\]]*\]/.test(result.text) ||
                       /[\u0600-\u06FF]/.test(result.text) ||
                       result.text.includes('[/');

    if (hasGarbage) {
      console.log('\n❌ GARBAGE DETECTED - adapter is INCOMPATIBLE');
    } else {
      console.log('\n✅ OUTPUT LOOKS GOOD');
    }
  }

  client.close();
}

main().catch(console.error);
