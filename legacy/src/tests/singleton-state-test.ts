/**
 * Singleton State Test
 *
 * Tests InferenceWorkerClient singleton behavior across multiple requests,
 * including error scenarios. The goal is to find why the singleton gets stuck
 * while fresh instances work fine.
 *
 * Run: npx tsx tests/singleton-state-test.ts
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

async function testSingleton(): Promise<void> {
  console.log('\n' + '='.repeat(70));
  console.log('🔬 SINGLETON STATE TEST');
  console.log('='.repeat(70) + '\n');

  // Get singleton instance
  const client = InferenceWorkerClient.instance;
  const MODEL_ID = 'Qwen/Qwen2-1.5B-Instruct';

  // Test 1: Basic ping with singleton
  log('INFO', 'Test 1: Singleton ping');
  try {
    const pingResult = await client.ping();
    log('PASS', `Singleton ping: ${pingResult.worker} v${pingResult.version}`);
  } catch (error) {
    log('FAIL', `Singleton ping failed: ${error}`);
    log('WARN', 'This suggests the singleton is already in a bad state!');
    return;
  }

  // Test 2: Load model
  log('INFO', 'Test 2: Load model');
  try {
    await client.loadModel(MODEL_ID);
    log('PASS', 'Model loaded');
  } catch (error) {
    log('FAIL', `Load failed: ${error}`);
  }

  // Test 3: Generate text
  log('INFO', 'Test 3: Generate text');
  const genStart = Date.now();
  try {
    const result = await client.generate({
      modelId: MODEL_ID,
      prompt: 'Say hello',
      maxTokens: 10,
    });
    log('PASS', `Generated in ${Date.now() - genStart}ms: "${result.text.substring(0, 30)}..."`);
  } catch (error) {
    log('FAIL', `Generation failed: ${error}`);
  }

  // Test 4: Force close and try again (simulate connection loss)
  log('INFO', 'Test 4: Force close socket, then ping');
  client.close();

  try {
    const pingResult = await client.ping();
    log('PASS', `Reconnection works: ${pingResult.worker}`);
  } catch (error) {
    log('FAIL', `Post-close ping failed: ${error}`);
  }

  // Test 5: Concurrent requests after close
  log('INFO', 'Test 5: 3 concurrent pings after forced close');
  client.close();

  const concurrentPings = Array(3).fill(0).map((_, i) => {
    const start = Date.now();
    return client.ping()
      .then(() => {
        log('PASS', `Concurrent ping ${i} in ${Date.now() - start}ms`);
        return { success: true };
      })
      .catch((e) => {
        log('FAIL', `Concurrent ping ${i} failed: ${e}`);
        return { success: false };
      });
  });

  await Promise.all(concurrentPings);

  // Test 6: Simulate timeout by using very short timeout
  log('INFO', 'Test 6: Rapid-fire sequential requests');
  for (let i = 0; i < 5; i++) {
    const start = Date.now();
    try {
      await client.ping();
      log('PASS', `Sequential ${i} in ${Date.now() - start}ms`);
    } catch (error) {
      log('FAIL', `Sequential ${i} failed: ${error}`);
    }
  }

  // Test 7: Check internal state
  log('INFO', 'Test 7: Checking internal state...');

  // Use any to access private properties for debugging
  const clientAny = client as any;
  log('INFO', `  socket: ${clientAny.socket ? 'exists' : 'null'}`);
  log('INFO', `  socket.destroyed: ${clientAny.socket?.destroyed ?? 'N/A'}`);
  log('INFO', `  pendingResponse: ${clientAny.pendingResponse ? 'pending' : 'null'}`);
  log('INFO', `  requestQueue length: ${clientAny.requestQueue?.length ?? 'N/A'}`);
  log('INFO', `  isProcessingQueue: ${clientAny.isProcessingQueue ?? 'N/A'}`);

  // Test 8: Long generation to verify queue isn't blocked
  log('INFO', 'Test 8: Long generation after state check');
  try {
    const start = Date.now();
    const result = await client.generate({
      modelId: MODEL_ID,
      prompt: 'Count from 1 to 10:',
      maxTokens: 50,
    });
    log('PASS', `Long gen: ${result.generatedTokens} tokens in ${Date.now() - start}ms`);
  } catch (error) {
    log('FAIL', `Long gen failed: ${error}`);
  }

  // Final state check
  log('INFO', 'Final state check:');
  log('INFO', `  socket: ${clientAny.socket ? 'exists' : 'null'}`);
  log('INFO', `  pendingResponse: ${clientAny.pendingResponse ? 'STILL PENDING!' : 'null'}`);
  log('INFO', `  requestQueue length: ${clientAny.requestQueue?.length ?? 'N/A'}`);
  log('INFO', `  isProcessingQueue: ${clientAny.isProcessingQueue ?? 'N/A'}`);

  if (clientAny.pendingResponse) {
    log('FAIL', '⚠️  pendingResponse is NOT null - this is a bug!');
  }
  if (clientAny.requestQueue?.length > 0) {
    log('FAIL', '⚠️  requestQueue is NOT empty - requests are stuck!');
  }
  if (clientAny.isProcessingQueue) {
    log('FAIL', '⚠️  isProcessingQueue is true - queue is stuck!');
  }

  console.log('\n' + '='.repeat(70));
  log('INFO', 'Singleton state test complete');
  console.log('='.repeat(70) + '\n');

  client.close();
}

testSingleton().catch(console.error);
