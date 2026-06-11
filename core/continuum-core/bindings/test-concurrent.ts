#!/usr/bin/env tsx
/**
 * Test concurrent request handling with request IDs
 */

import { RustCoreIPCClient } from './RustCoreIPC';

async function main() {
	console.log('🧪 Testing concurrent requests with request IDs\n');

	const client = new RustCoreIPCClient('/tmp/continuum-core.sock');

	// Connect
	await client.connect();
	console.log('✅ Connected\n');

	// Test 1: Sequential requests (baseline)
	console.log('Test 1: Sequential requests');
	const start1 = performance.now();
	for (let i = 0; i < 10; i++) {
		await client.healthCheck();
	}
	const duration1 = performance.now() - start1;
	console.log(`  10 sequential requests: ${duration1.toFixed(2)}ms (${(duration1 / 10).toFixed(3)}ms each)\n`);

	// Test 2: Concurrent requests (10 parallel)
	console.log('Test 2: Concurrent requests (10 parallel)');
	const start2 = performance.now();
	await Promise.all(Array.from({ length: 10 }, () => client.healthCheck()));
	const duration2 = performance.now() - start2;
	console.log(`  10 concurrent requests: ${duration2.toFixed(2)}ms total\n`);

	// Test 3: 100 concurrent requests
	console.log('Test 3: 100 concurrent requests');
	const start3 = performance.now();
	const results = await Promise.all(Array.from({ length: 100 }, () => client.healthCheck()));
	const duration3 = performance.now() - start3;
	const allHealthy = results.every((r) => r === true);
	console.log(`  100 concurrent requests: ${duration3.toFixed(2)}ms total`);
	console.log(`  All responses valid: ${allHealthy ? '✅' : '❌'}\n`);

	// Summary
	console.log('Summary:');
	console.log(`  Sequential (10):   ${duration1.toFixed(2)}ms (${(duration1 / 10).toFixed(3)}ms each)`);
	console.log(`  Concurrent (10):   ${duration2.toFixed(2)}ms (${(duration2 / 10).toFixed(3)}ms amortized)`);
	console.log(`  Concurrent (100):  ${duration3.toFixed(2)}ms (${(duration3 / 100).toFixed(3)}ms amortized)`);
	console.log(`  Speedup (10):      ${(duration1 / duration2).toFixed(1)}x`);
	console.log(`  Speedup (100):     ${(duration1 * 10 / duration3).toFixed(1)}x\n`);

	if (allHealthy) {
		console.log('✅ Concurrent requests working correctly!');
	} else {
		console.log('❌ Some concurrent requests failed');
	}

	client.disconnect();
}

main().catch((e) => {
	console.error('❌ Test failed:', e);
	process.exit(1);
});
