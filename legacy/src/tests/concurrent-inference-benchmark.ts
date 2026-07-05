/**
 * Concurrent Inference Benchmark
 *
 * Measures throughput with parallel requests to verify worker pool utilization.
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

const CONCURRENT_REQUESTS = 4; // Match worker pool size
const PROMPT = `<|begin_of_text|><|start_header_id|>system<|end_header_id|>Be brief.<|eot_id|><|start_header_id|>user<|end_header_id|>What is 2+2?<|eot_id|><|start_header_id|>assistant<|end_header_id|>`;

async function benchmark() {
  const client = new InferenceGrpcClient();

  console.log('🚀 Concurrent Inference Benchmark');
  console.log(`   Requests: ${CONCURRENT_REQUESTS} parallel`);
  console.log('');

  // Warmup
  console.log('⏳ Warming up...');
  await client.generate(PROMPT, { maxTokens: 5, temperature: 0.1 });

  // Sequential baseline
  console.log('📊 Sequential (baseline)...');
  const seqStart = performance.now();
  for (let i = 0; i < CONCURRENT_REQUESTS; i++) {
    await client.generate(PROMPT, { maxTokens: 5, temperature: 0.1 });
  }
  const seqTime = performance.now() - seqStart;
  console.log(`   Sequential: ${seqTime.toFixed(0)}ms total (${(seqTime / CONCURRENT_REQUESTS).toFixed(0)}ms avg)`);

  // Parallel
  console.log('📊 Parallel (worker pool)...');
  const parStart = performance.now();
  const promises = [];
  for (let i = 0; i < CONCURRENT_REQUESTS; i++) {
    promises.push(client.generate(PROMPT, { maxTokens: 5, temperature: 0.1 }));
  }
  await Promise.all(promises);
  const parTime = performance.now() - parStart;
  console.log(`   Parallel: ${parTime.toFixed(0)}ms total (${(parTime / CONCURRENT_REQUESTS).toFixed(0)}ms avg)`);

  // Results
  console.log('');
  console.log('📈 Results:');
  console.log(`   Speedup: ${(seqTime / parTime).toFixed(2)}x`);
  console.log(`   Parallel efficiency: ${((seqTime / parTime) / CONCURRENT_REQUESTS * 100).toFixed(0)}%`);

  // Higher concurrency test
  console.log('');
  console.log('📊 High concurrency (8 requests)...');
  const highStart = performance.now();
  const highPromises = [];
  for (let i = 0; i < 8; i++) {
    highPromises.push(client.generate(PROMPT, { maxTokens: 5, temperature: 0.1 }));
  }
  await Promise.all(highPromises);
  const highTime = performance.now() - highStart;
  console.log(`   8 parallel: ${highTime.toFixed(0)}ms total (${(highTime / 8).toFixed(0)}ms avg)`);
  console.log(`   Throughput: ${(8 / (highTime / 1000)).toFixed(1)} req/s`);

  process.exit(0);
}

benchmark().catch(console.error);
