/**
 * gRPC Stress Test - Test concurrent inference requests
 *
 * Verifies that the gRPC server handles multiple simultaneous requests
 * without the blocking/stuck behavior of the old Unix socket client.
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

async function runStressTest(): Promise<void> {
  const client = new InferenceGrpcClient();

  console.log('=== gRPC Concurrent Request Stress Test ===\n');

  // Test 1: Ping test
  console.log('1. Ping test...');
  try {
    const pong = await client.ping();
    console.log(`   ✅ ${pong.message}`);
  } catch (err) {
    console.log(`   ❌ Ping failed: ${err}`);
    process.exit(1);
  }

  // Test 2: Sequential requests (baseline)
  console.log('\n2. Sequential requests (baseline)...');
  const seqStart = Date.now();
  for (let i = 0; i < 3; i++) {
    try {
      const result = await client.generate('Qwen/Qwen2-1.5B-Instruct', 'Say hello', {
        maxTokens: 10,
        timeoutMs: 30000,
      });
      console.log(`   Request ${i + 1}: ${result.tokens} tokens in ${result.durationMs}ms`);
    } catch (err) {
      console.log(`   ❌ Request ${i + 1} failed: ${err}`);
    }
  }
  const seqDuration = Date.now() - seqStart;
  console.log(`   Total sequential time: ${seqDuration}ms`);

  // Test 3: Concurrent requests (the real test)
  console.log('\n3. Concurrent requests (5 at once)...');
  const concStart = Date.now();

  const promises = Array.from({ length: 5 }, (_, i) =>
    client.generate('Qwen/Qwen2-1.5B-Instruct', `Count to ${i + 1}`, {
      maxTokens: 20,
      timeoutMs: 60000,
    })
      .then(result => ({
        id: i + 1,
        success: true,
        tokens: result.tokens,
        durationMs: result.durationMs,
        text: result.text.slice(0, 50),
      }))
      .catch(err => ({
        id: i + 1,
        success: false,
        error: String(err)
      }))
  );

  const results = await Promise.all(promises);
  const concDuration = Date.now() - concStart;

  for (const r of results) {
    if (r.success) {
      console.log(`   Request ${r.id}: ✅ ${r.tokens} tokens in ${r.durationMs}ms - "${r.text}..."`);
    } else {
      console.log(`   Request ${r.id}: ❌ ${r.error}`);
    }
  }
  console.log(`   Total concurrent time: ${concDuration}ms`);

  const successCount = results.filter(r => r.success).length;
  console.log(`\n   Result: ${successCount}/5 requests succeeded`);

  if (successCount === 5) {
    console.log('   ✅ All concurrent requests completed!');
    console.log(`   Speedup: ${(seqDuration / concDuration).toFixed(2)}x faster than sequential`);
  } else {
    console.log('   ⚠️  Some requests failed - check server logs');
  }

  // Test 4: Rapid fire (test for any buildup/blocking)
  console.log('\n4. Rapid fire test (10 quick requests)...');
  const rapidStart = Date.now();
  const rapidPromises = Array.from({ length: 10 }, (_, i) =>
    client.generate('Qwen/Qwen2-1.5B-Instruct', 'Hi', { maxTokens: 5, timeoutMs: 60000 })
      .then(() => ({ id: i + 1, success: true }))
      .catch(err => ({ id: i + 1, success: false, error: String(err) }))
  );

  const rapidResults = await Promise.all(rapidPromises);
  const rapidDuration = Date.now() - rapidStart;
  const rapidSuccess = rapidResults.filter(r => r.success).length;
  console.log(`   ${rapidSuccess}/10 completed in ${rapidDuration}ms (${(rapidDuration / 10).toFixed(0)}ms avg)`);

  client.close();
  console.log('\n=== Stress test complete ===');
}

runStressTest().catch(console.error);
