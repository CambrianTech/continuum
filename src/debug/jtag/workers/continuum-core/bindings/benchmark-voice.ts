#!/usr/bin/env tsx
/**
 * Performance benchmark for voice orchestration
 *
 * Tests:
 * 1. IPC latency (baseline overhead)
 * 2. Orchestrator selection latency
 * 3. Concurrent request handling
 * 4. Message size impact
 * 5. Memory usage
 *
 * Goal: Sub-1ms for 99th percentile
 */

import { RustCoreIPCClient } from './RustCoreIPC';

interface BenchmarkResult {
	name: string;
	iterations: number;
	mean: number;
	median: number;
	p95: number;
	p99: number;
	min: number;
	max: number;
}

function percentile(values: number[], p: number): number {
	const sorted = values.slice().sort((a, b) => a - b);
	const index = Math.floor(sorted.length * p);
	return sorted[index];
}

function analyze(name: string, timings: number[]): BenchmarkResult {
	const sorted = timings.slice().sort((a, b) => a - b);
	return {
		name,
		iterations: timings.length,
		mean: timings.reduce((a, b) => a + b, 0) / timings.length,
		median: percentile(timings, 0.5),
		p95: percentile(timings, 0.95),
		p99: percentile(timings, 0.99),
		min: sorted[0],
		max: sorted[sorted.length - 1],
	};
}

function printResult(result: BenchmarkResult): void {
	console.log(`\n${result.name}`);
	console.log(`  Iterations: ${result.iterations}`);
	console.log(`  Mean:       ${result.mean.toFixed(3)}ms`);
	console.log(`  Median:     ${result.median.toFixed(3)}ms`);
	console.log(`  95th %ile:  ${result.p95.toFixed(3)}ms`);
	console.log(`  99th %ile:  ${result.p99.toFixed(3)}ms`);
	console.log(`  Min:        ${result.min.toFixed(3)}ms`);
	console.log(`  Max:        ${result.max.toFixed(3)}ms`);
}

async function main() {
	console.log('🦀 Voice Orchestration Performance Benchmark\n');
	console.log('Target: Sub-1ms for 99th percentile\n');

	const client = new RustCoreIPCClient('/tmp/continuum-core.sock');

	// Connect
	console.log('Connecting to server...');
	await client.connect();
	console.log('✅ Connected\n');

	// Setup session
	const sessionId = '550e8400-e29b-41d4-a716-446655440000';
	const roomId = '550e8400-e29b-41d4-a716-446655440001';

	await client.voiceRegisterSession(sessionId, roomId, [
		{
			user_id: '550e8400-e29b-41d4-a716-446655440002',
			display_name: 'Joel',
			participant_type: 'human',
			expertise: [],
		},
		{
			user_id: '550e8400-e29b-41d4-a716-446655440003',
			display_name: 'Helper AI',
			participant_type: 'persona',
			expertise: ['typescript', 'rust'],
		},
		{
			user_id: '550e8400-e29b-41d4-a716-446655440004',
			display_name: 'Teacher AI',
			participant_type: 'persona',
			expertise: ['education', 'mentoring'],
		},
	]);

	// Benchmark 1: Health check (minimal IPC overhead)
	console.log('Benchmark 1: Health Check (IPC baseline)');
	const healthTimings: number[] = [];
	for (let i = 0; i < 1000; i++) {
		const start = performance.now();
		await client.healthCheck();
		healthTimings.push(performance.now() - start);
	}
	printResult(analyze('Health Check (IPC baseline)', healthTimings));

	// Benchmark 2: Utterance processing (question with keyword match)
	console.log('\n\nBenchmark 2: Utterance Processing (keyword match)');
	const utteranceTimings: number[] = [];
	for (let i = 0; i < 1000; i++) {
		const start = performance.now();
		await client.voiceOnUtterance({
			session_id: sessionId,
			speaker_id: '550e8400-e29b-41d4-a716-446655440002',
			speaker_name: 'Joel',
			speaker_type: 'human',
			transcript: 'How do I use Rust generics?',
			confidence: 0.95,
			timestamp: Date.now(),
		});
		utteranceTimings.push(performance.now() - start);
	}
	printResult(analyze('Utterance Processing (keyword match)', utteranceTimings));

	// Benchmark 3: Utterance processing (no match, round-robin)
	console.log('\n\nBenchmark 3: Utterance Processing (round-robin)');
	const roundRobinTimings: number[] = [];
	for (let i = 0; i < 1000; i++) {
		const start = performance.now();
		await client.voiceOnUtterance({
			session_id: sessionId,
			speaker_id: '550e8400-e29b-41d4-a716-446655440002',
			speaker_name: 'Joel',
			speaker_type: 'human',
			transcript: 'What is the meaning of life?',
			confidence: 0.95,
			timestamp: Date.now(),
		});
		roundRobinTimings.push(performance.now() - start);
	}
	printResult(analyze('Utterance Processing (round-robin)', roundRobinTimings));

	// Benchmark 4: Utterance processing (statement, no responder)
	console.log('\n\nBenchmark 4: Statement Processing (no responder)');
	const statementTimings: number[] = [];
	for (let i = 0; i < 1000; i++) {
		const start = performance.now();
		await client.voiceOnUtterance({
			session_id: sessionId,
			speaker_id: '550e8400-e29b-41d4-a716-446655440002',
			speaker_name: 'Joel',
			speaker_type: 'human',
			transcript: 'The weather is nice today.',
			confidence: 0.95,
			timestamp: Date.now(),
		});
		statementTimings.push(performance.now() - start);
	}
	printResult(analyze('Statement Processing (no responder)', statementTimings));

	// Benchmark 5: TTS routing check
	console.log('\n\nBenchmark 5: TTS Routing Check');
	const ttsTimings: number[] = [];
	for (let i = 0; i < 1000; i++) {
		const start = performance.now();
		await client.voiceShouldRouteTts(sessionId, '550e8400-e29b-41d4-a716-446655440003');
		ttsTimings.push(performance.now() - start);
	}
	printResult(analyze('TTS Routing Check', ttsTimings));

	// Benchmark 6: Message size impact (long transcript)
	console.log('\n\nBenchmark 6: Long Transcript Impact');
	const longText = 'How do I '.repeat(100) + 'use Rust?';
	const longTimings: number[] = [];
	for (let i = 0; i < 1000; i++) {
		const start = performance.now();
		await client.voiceOnUtterance({
			session_id: sessionId,
			speaker_id: '550e8400-e29b-41d4-a716-446655440002',
			speaker_name: 'Joel',
			speaker_type: 'human',
			transcript: longText,
			confidence: 0.95,
			timestamp: Date.now(),
		});
		longTimings.push(performance.now() - start);
	}
	printResult(analyze(`Long Transcript (${longText.length} chars)`, longTimings));

	// Benchmark 7: Concurrent requests (10 parallel)
	console.log('\n\nBenchmark 7: Concurrent Requests (10 parallel)');
	const concurrentTimings: number[] = [];
	for (let i = 0; i < 100; i++) {
		const start = performance.now();
		await Promise.all(
			Array.from({ length: 10 }, () =>
				client.voiceOnUtterance({
					session_id: sessionId,
					speaker_id: '550e8400-e29b-41d4-a716-446655440002',
					speaker_name: 'Joel',
					speaker_type: 'human',
					transcript: 'How do I use Rust?',
					confidence: 0.95,
					timestamp: Date.now(),
				})
			)
		);
		concurrentTimings.push(performance.now() - start);
	}
	printResult(analyze('10 Concurrent Requests (total time)', concurrentTimings));

	// Summary
	console.log('\n\n' + '='.repeat(60));
	console.log('SUMMARY');
	console.log('='.repeat(60));

	const results = [
		analyze('Health Check (IPC baseline)', healthTimings),
		analyze('Utterance (keyword)', utteranceTimings),
		analyze('Utterance (round-robin)', roundRobinTimings),
		analyze('Statement (no responder)', statementTimings),
		analyze('TTS routing', ttsTimings),
		analyze('Long transcript', longTimings),
	];

	console.log('\nP99 Latencies:');
	results.forEach((r) => {
		const status = r.p99 < 1.0 ? '✅' : '⚠️ ';
		console.log(`  ${status} ${r.name.padEnd(40)} ${r.p99.toFixed(3)}ms`);
	});

	console.log('\n\nIPC Overhead Analysis:');
	const ipcBaseline = results[0].mean;
	console.log(`  Pure IPC (health check):        ${ipcBaseline.toFixed(3)}ms`);
	console.log(`  Utterance processing:           ${results[1].mean.toFixed(3)}ms`);
	console.log(`  Orchestrator logic overhead:    ${(results[1].mean - ipcBaseline).toFixed(3)}ms`);

	console.log('\n\nConcurrent Performance:');
	const concurrentResult = analyze('10 Concurrent', concurrentTimings);
	console.log(`  Total time (10 requests):       ${concurrentResult.mean.toFixed(3)}ms`);
	console.log(`  Per-request (amortized):        ${(concurrentResult.mean / 10).toFixed(3)}ms`);

	// Cleanup
	client.disconnect();
	console.log('\n✅ Benchmark complete!\n');
}

main().catch((e) => {
	console.error('❌ Benchmark failed:', e);
	process.exit(1);
});
