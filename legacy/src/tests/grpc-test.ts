/**
 * Test the gRPC client end-to-end
 */

import { InferenceGrpcClient } from '../system/core/services/InferenceGrpcClient';

async function main() {
  console.log('=== Testing InferenceGrpcClient ===\n');

  const client = new InferenceGrpcClient('localhost:50051');

  // Test ping
  console.log('1. Testing ping...');
  try {
    const pong = await client.ping();
    console.log(`   Ping response: ${pong.message} (timestamp: ${pong.timestamp})`);
    console.log('   PASS\n');
  } catch (err) {
    console.log(`   FAIL: ${err}`);
    process.exit(1);
  }

  // Test generate
  console.log('2. Testing generate...');
  try {
    const result = await client.generate('Hello world', 50, {
      timeoutMs: 30000,
      onProgress: (p) => {
        console.log(`   Progress: ${p.tokensGenerated}/${p.tokensTotal}`);
      },
    });
    console.log(`   Result: "${result.text.substring(0, 100)}..."`);
    console.log(`   Tokens: ${result.tokens}, Duration: ${result.durationMs}ms`);
    console.log('   PASS\n');
  } catch (err) {
    console.log(`   FAIL: ${err}`);
    process.exit(1);
  }

  // Test cancellation
  console.log('3. Testing cancellation...');
  try {
    const controller = new AbortController();

    // Cancel after 500ms
    setTimeout(() => {
      console.log('   Cancelling...');
      controller.abort();
    }, 500);

    await client.generate('Count to a million', 1000, {
      timeoutMs: 60000,
      signal: controller.signal,
      onProgress: (p) => {
        console.log(`   Progress before cancel: ${p.tokensGenerated}/${p.tokensTotal}`);
      },
    });
    console.log('   FAIL: Should have been cancelled');
    process.exit(1);
  } catch (err: any) {
    if (err.message === 'Generation cancelled') {
      console.log('   Cancelled successfully!');
      console.log('   PASS\n');
    } else {
      console.log(`   FAIL: Wrong error: ${err}`);
      process.exit(1);
    }
  }

  // Test concurrent requests
  console.log('4. Testing concurrent requests...');
  try {
    const start = Date.now();
    const results = await Promise.all([
      client.generate('Request 1', 30),
      client.generate('Request 2', 30),
      client.generate('Request 3', 30),
    ]);
    const duration = Date.now() - start;
    console.log(`   All 3 completed in ${duration}ms`);
    console.log(`   Results: ${results.map(r => r.tokens).join(', ')} tokens`);
    console.log('   PASS\n');
  } catch (err) {
    console.log(`   FAIL: ${err}`);
    process.exit(1);
  }

  console.log('=== ALL TESTS PASSED ===');
  client.close();
  process.exit(0);
}

main().catch(console.error);
