/**
 * Layer 4: TypeScript IPC Client Integration Test
 *
 * Tests the RustCoreIPCClient against a running continuum-core server.
 * Validates: connection, health check, voice synthesis with binary framing.
 *
 * Prerequisites:
 *   - continuum-core server running (npm start or start-workers.sh)
 *   - Kokoro model files in models/kokoro/
 *
 * Run with: npx tsx tests/integration/ipc-client-tts.test.ts
 */

import { RustCoreIPCClient } from '../../workers/continuum-core/bindings/RustCoreIPC';
import * as fs from 'fs';

const SOCKET_PATH = '/tmp/continuum-core.sock';

interface TestResult {
	name: string;
	passed: boolean;
	durationMs: number;
	detail: string;
}

const results: TestResult[] = [];

function assert(condition: boolean, message: string): void {
	if (!condition) throw new Error(`Assertion failed: ${message}`);
}

async function runTest(name: string, fn: () => Promise<string>): Promise<void> {
	const start = performance.now();
	try {
		const detail = await fn();
		const durationMs = performance.now() - start;
		results.push({ name, passed: true, durationMs, detail });
		console.log(`  ✅ ${name} (${durationMs.toFixed(1)}ms) — ${detail}`);
	} catch (e) {
		const durationMs = performance.now() - start;
		const detail = (e as Error).message;
		results.push({ name, passed: false, durationMs, detail });
		console.log(`  ❌ ${name} (${durationMs.toFixed(1)}ms) — ${detail}`);
	}
}

async function main() {
	console.log('=== Layer 4: TypeScript IPC Client Integration Tests ===\n');

	// Precondition: socket exists
	if (!fs.existsSync(SOCKET_PATH)) {
		console.error(`❌ Socket not found: ${SOCKET_PATH}`);
		console.error('   Start the system first: npm start');
		process.exit(1);
	}

	const client = new RustCoreIPCClient(SOCKET_PATH);

	// ====================================================================
	// Test 1: Connect to IPC server
	// ====================================================================
	await runTest('Connect to continuum-core IPC server', async () => {
		await client.connect();
		return 'Connected via Unix socket';
	});

	// ====================================================================
	// Test 2: Health check
	// ====================================================================
	await runTest('Health check round-trip', async () => {
		const healthy = await client.healthCheck();
		assert(healthy === true, 'Health check should return true');
		return 'Server is healthy';
	});

	// ====================================================================
	// Test 3: Voice synthesize with binary framing
	// ====================================================================
	await runTest('Voice synthesize returns binary PCM audio', async () => {
		const result = await client.voiceSynthesize('Hello, this is a test.', 'af', 'kokoro');

		assert(result.audio.length > 0, 'Audio buffer should not be empty');
		assert(result.sampleRate === 16000, `Sample rate should be 16000, got ${result.sampleRate}`);
		assert(result.numSamples > 100, `Should have >100 samples, got ${result.numSamples}`);
		assert(result.durationMs > 50, `Should be >50ms, got ${result.durationMs}`);
		assert(result.audio.length === result.numSamples * 2, 'Buffer should be 2 bytes per sample (i16)');

		// Verify PCM data is valid audio (not silence)
		const samples: number[] = [];
		for (let i = 0; i < result.audio.length; i += 2) {
			samples.push(result.audio.readInt16LE(i));
		}
		const maxAmp = Math.max(...samples.map(s => Math.abs(s)));
		assert(maxAmp > 100, `Audio should not be silence, max amplitude: ${maxAmp}`);

		return `${result.numSamples} samples, ${result.sampleRate}Hz, ${result.durationMs}ms, adapter: ${result.adapter}, max amp: ${maxAmp}`;
	});

	// ====================================================================
	// Test 4: Voice synthesize with silence adapter (always works)
	// ====================================================================
	await runTest('Voice synthesize with silence adapter', async () => {
		const result = await client.voiceSynthesize('Test silence', undefined, 'silence');

		assert(result.audio.length > 0, 'Should produce audio buffer');
		assert(result.sampleRate === 16000, `Sample rate should be 16000, got ${result.sampleRate}`);

		// Silence adapter should produce all zeros
		const samples: number[] = [];
		for (let i = 0; i < result.audio.length; i += 2) {
			samples.push(result.audio.readInt16LE(i));
		}
		const allZero = samples.every(s => s === 0);
		assert(allZero, 'Silence adapter should produce all-zero samples');

		return `${result.numSamples} samples (all silence), ${result.durationMs}ms`;
	});

	// ====================================================================
	// Test 5: Voice synthesize error handling (empty text)
	// ====================================================================
	await runTest('Voice synthesize rejects empty text', async () => {
		try {
			await client.voiceSynthesize('', 'af', 'kokoro');
			throw new Error('Should have thrown for empty text');
		} catch (e) {
			const msg = (e as Error).message;
			assert(msg.includes('empty') || msg.includes('Invalid') || msg.includes('failed') || msg.includes('Failed'),
				`Error should mention invalid/empty text: ${msg}`);
			return `Correctly rejected: ${msg.substring(0, 80)}`;
		}
	});

	// ====================================================================
	// Test 6: Multiple sequential requests (connection reuse)
	// ====================================================================
	await runTest('Multiple sequential requests on same connection', async () => {
		const checks = await Promise.all([
			client.healthCheck(),
			client.healthCheck(),
			client.healthCheck(),
		]);
		assert(checks.every(c => c === true), 'All health checks should succeed');
		return '3 concurrent health checks succeeded';
	});

	// ====================================================================
	// Test 7: Latency benchmark
	// ====================================================================
	await runTest('Health check latency benchmark (10 calls)', async () => {
		const times: number[] = [];
		for (let i = 0; i < 10; i++) {
			const start = performance.now();
			await client.healthCheck();
			times.push(performance.now() - start);
		}
		const avg = times.reduce((a, b) => a + b, 0) / times.length;
		const min = Math.min(...times);
		const max = Math.max(...times);
		return `avg: ${avg.toFixed(2)}ms, min: ${min.toFixed(2)}ms, max: ${max.toFixed(2)}ms`;
	});

	// ====================================================================
	// Cleanup
	// ====================================================================
	client.disconnect();

	// Summary
	console.log('\n=== Results ===');
	const passed = results.filter(r => r.passed).length;
	const failed = results.filter(r => !r.passed).length;
	console.log(`${passed} passed, ${failed} failed out of ${results.length} tests`);

	if (failed > 0) {
		console.log('\nFailed tests:');
		for (const r of results.filter(r => !r.passed)) {
			console.log(`  ❌ ${r.name}: ${r.detail}`);
		}
		process.exit(1);
	}
}

main().catch(e => {
	console.error('Fatal error:', e);
	process.exit(1);
});
