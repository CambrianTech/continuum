/**
 * TypeScript Communication Bug Test
 *
 * Tests the InferenceWorkerClient to find the communication bug.
 * The isolated Rust test showed the worker is fine - issue is in TS.
 *
 * Run: npx tsx tests/ts-comms-bug-test.ts
 */

import { InferenceWorkerClient } from '../system/core/services/InferenceWorkerClient';

const GREEN = '\x1b[32m';
const RED = '\x1b[31m';
const YELLOW = '\x1b[33m';
const CYAN = '\x1b[36m';
const RESET = '\x1b[0m';

function log(level: 'INFO' | 'PASS' | 'FAIL' | 'WARN', msg: string): void {
  const colors: Record<string, string> = { INFO: CYAN, PASS: GREEN, FAIL: RED, WARN: YELLOW };
  const timestamp = new Date().toISOString().slice(11, 23);
  console.log(`${colors[level]}[${timestamp}] [${level}]${RESET} ${msg}`);
}

async function runTests(): Promise<void> {
  console.log('\n' + '='.repeat(70));
  console.log('🔬 TYPESCRIPT COMMUNICATION BUG TEST');
  console.log('='.repeat(70) + '\n');

  const client = InferenceWorkerClient.instance;
  const MODEL_ID = 'Qwen/Qwen2-1.5B-Instruct';

  // Test 1: Basic ping
  log('INFO', 'Test 1: Single ping');
  try {
    const pingResult = await client.ping();
    log('PASS', `Ping: ${JSON.stringify(pingResult)}`);
  } catch (error) {
    log('FAIL', `Ping failed: ${error}`);
    return;
  }

  // Test 2: Ensure model loaded
  log('INFO', 'Test 2: Load model');
  try {
    const loadResult = await client.loadModel(MODEL_ID);
    log('PASS', `Load: ${JSON.stringify(loadResult)}`);
  } catch (error) {
    log('FAIL', `Load failed: ${error}`);
    return;
  }

  // Test 3: Sequential generations
  log('INFO', 'Test 3: Sequential generations (2x)');
  for (let i = 0; i < 2; i++) {
    try {
      const start = Date.now();
      const result = await client.generate({
        modelId: MODEL_ID,
        prompt: `Test ${i}: Say hello`,
        maxTokens: 10,
        temperature: 0.1,
      });
      log('PASS', `Gen ${i}: "${result.text.substring(0, 30)}..." (${Date.now() - start}ms)`);
    } catch (error) {
      log('FAIL', `Gen ${i} failed: ${error}`);
    }
  }

  // Test 4: Concurrent pings (should all succeed)
  log('INFO', 'Test 4: Concurrent pings (5x)');
  const pingPromises = Array(5).fill(0).map((_, i) =>
    client.ping()
      .then(() => log('PASS', `Concurrent ping ${i} succeeded`))
      .catch((e) => log('FAIL', `Concurrent ping ${i} failed: ${e}`))
  );
  await Promise.all(pingPromises);

  // Test 5: Concurrent generations (THIS is where bugs usually show)
  log('INFO', 'Test 5: Concurrent generations (3x) - THE CRITICAL TEST');
  log('WARN', 'If the queue is broken, requests will hang or fail...');

  const genPromises = Array(3).fill(0).map((_, i) => {
    const start = Date.now();
    return client.generate({
      modelId: MODEL_ID,
      prompt: `Concurrent test ${i}: Say your number`,
      maxTokens: 10,
      temperature: 0.1,
    })
      .then((result) => {
        log('PASS', `Concurrent gen ${i}: "${result.text.substring(0, 20)}..." (${Date.now() - start}ms)`);
        return { success: true, i };
      })
      .catch((error) => {
        log('FAIL', `Concurrent gen ${i} FAILED after ${Date.now() - start}ms: ${error}`);
        return { success: false, i, error: String(error) };
      });
  });

  const results = await Promise.all(genPromises);
  const failed = results.filter((r) => !r.success);

  if (failed.length > 0) {
    log('FAIL', `${failed.length}/3 concurrent generations failed!`);
    for (const f of failed) {
      log('FAIL', `  Gen ${f.i}: ${f.error}`);
    }
  } else {
    log('PASS', 'All concurrent generations succeeded');
  }

  // Test 6: Rapid fire ping/generate interleaving
  log('INFO', 'Test 6: Interleaved ping+generate (stress test)');
  const interleavedPromises: Promise<void>[] = [];
  for (let i = 0; i < 6; i++) {
    if (i % 2 === 0) {
      interleavedPromises.push(
        client.ping()
          .then(() => log('INFO', `Interleaved ping ${i / 2} done`))
          .catch((e) => log('FAIL', `Interleaved ping ${i / 2} failed: ${e}`))
      );
    } else {
      interleavedPromises.push(
        client.generate({ modelId: MODEL_ID, prompt: 'quick', maxTokens: 5 })
          .then(() => log('INFO', `Interleaved gen ${Math.floor(i / 2)} done`))
          .catch((e) => log('FAIL', `Interleaved gen ${Math.floor(i / 2)} failed: ${e}`))
      );
    }
  }
  await Promise.all(interleavedPromises);

  // Test 7: Binary protocol
  log('INFO', 'Test 7: Binary protocol generation');
  try {
    const start = Date.now();
    const result = await client.generateBinary(
      MODEL_ID,
      'Binary test: say hello with newlines\n\nThis has special chars: <>',
      10,
      0.1
    );
    log('PASS', `Binary gen: "${result.text.substring(0, 30)}..." (${Date.now() - start}ms)`);
  } catch (error) {
    log('FAIL', `Binary gen failed: ${error}`);
  }

  // Test 8: Long-running generation (might reveal timeout issues)
  log('INFO', 'Test 8: Longer generation (100 tokens)');
  try {
    const start = Date.now();
    const result = await client.generate({
      modelId: MODEL_ID,
      prompt: 'Write a haiku about programming:',
      maxTokens: 100,
      temperature: 0.7,
    });
    log('PASS', `Long gen: ${result.generatedTokens} tokens in ${Date.now() - start}ms`);
  } catch (error) {
    log('FAIL', `Long gen failed: ${error}`);
  }

  // Test 9: Repeated socket reconnection
  log('INFO', 'Test 9: Force reconnection');
  client.close();
  try {
    await client.ping();
    log('PASS', 'Reconnection successful');
  } catch (error) {
    log('FAIL', `Reconnection failed: ${error}`);
  }

  // Test 10: Stress test - many rapid requests
  log('INFO', 'Test 10: Stress test - 10 rapid requests');
  const stressStart = Date.now();
  const stressPromises = Array(10).fill(0).map((_, i) =>
    client.ping().then(() => i).catch(() => -1)
  );
  const stressResults = await Promise.all(stressPromises);
  const stressSuccess = stressResults.filter((r) => r >= 0).length;
  log(stressSuccess === 10 ? 'PASS' : 'FAIL',
    `Stress: ${stressSuccess}/10 succeeded in ${Date.now() - stressStart}ms`);

  console.log('\n' + '='.repeat(70));
  log('INFO', 'Tests complete!');
  console.log('='.repeat(70) + '\n');

  client.close();
}

runTests().catch(console.error);
