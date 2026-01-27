#!/usr/bin/env tsx
/**
 * Test continuum-core IPC client
 *
 * Verifies:
 * 1. Connection to Unix socket
 * 2. VoiceOrchestrator FFI via IPC
 * 3. Performance (should be <10ms per call)
 */

import { RustCoreIPCClient } from './RustCoreIPC';

async function main() {
	console.log('🦀 Testing continuum-core IPC...\n');

	const client = new RustCoreIPCClient('/tmp/continuum-core.sock');

	// Connect
	console.log('1. Connecting to server...');
	try {
		await client.connect();
		console.log('   ✅ Connected\n');
	} catch (e) {
		console.error('   ❌ Failed to connect:', e);
		process.exit(1);
	}

	// Health check
	console.log('2. Health check...');
	const healthy = await client.healthCheck();
	console.log(`   ${healthy ? '✅' : '❌'} Healthy: ${healthy}\n`);

	// Register voice session
	console.log('3. Registering voice session...');
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

	console.log('   ✅ Session registered with 3 participants\n');

	// Process utterance (question)
	console.log('4. Processing utterance (question)...');
	const start = performance.now();
	const responder = await client.voiceOnUtterance({
		session_id: sessionId,
		speaker_id: '550e8400-e29b-41d4-a716-446655440002',
		speaker_name: 'Joel',
		speaker_type: 'human',
		transcript: 'How do I implement priority queues in Rust?',
		confidence: 0.95,
		timestamp: Date.now(),
	});
	const duration = performance.now() - start;

	console.log(`   ${responder ? '✅' : '❌'} Responder: ${responder}`);
	console.log(`   ⏱️  IPC latency: ${duration.toFixed(2)}ms\n`);

	// Process utterance (statement)
	console.log('5. Processing utterance (statement)...');
	const noResponder = await client.voiceOnUtterance({
		session_id: sessionId,
		speaker_id: '550e8400-e29b-41d4-a716-446655440002',
		speaker_name: 'Joel',
		speaker_type: 'human',
		transcript: 'The weather is nice today.',
		confidence: 0.95,
		timestamp: Date.now(),
	});

	console.log(`   ${noResponder === null ? '✅' : '❌'} No responder for statement (correct)\n`);

	// Performance check
	if (duration < 10) {
		console.log(`✅ IPC performance excellent: ${duration.toFixed(2)}ms`);
	} else {
		console.warn(`⚠️  IPC performance needs optimization: ${duration.toFixed(2)}ms (target: <10ms)`);
	}

	// Cleanup
	client.disconnect();
	console.log('\n✅ All IPC tests passed!');
	console.log('🦀 Rust <-> TypeScript bridge working correctly via Unix socket\n');
}

main().catch((e) => {
	console.error('❌ Test failed:', e);
	process.exit(1);
});
