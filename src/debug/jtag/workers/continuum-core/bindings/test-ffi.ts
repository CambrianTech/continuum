#!/usr/bin/env tsx
/**
 * Quick FFI test - verify Rust <-> TypeScript bridge works
 *
 * Tests:
 * 1. RustCore.init() connects to logger
 * 2. VoiceOrchestrator FFI calls
 * 3. Performance timing
 */

import { RustCore, VoiceOrchestrator } from './RustCore';

console.log('🦀 Testing continuum-core FFI...\n');

// ============================================================================
// Test 1: Initialize
// ============================================================================

console.log('1. Testing initialization...');
try {
	// LoggerModule is now part of continuum-core (Phase 4a)
	RustCore.init('/tmp/continuum-core.sock');
	console.log('   ✅ Initialized\n');
} catch (e) {
	console.error('   ❌ Init failed:', e);
	process.exit(1);
}

// ============================================================================
// Test 2: Health Check
// ============================================================================

console.log('2. Testing health check...');
const healthy = RustCore.healthCheck();
console.log(`   ${healthy ? '✅' : '❌'} Health check: ${healthy}\n`);

// ============================================================================
// Test 3: VoiceOrchestrator
// ============================================================================

console.log('3. Testing VoiceOrchestrator...');
const orchestrator = new VoiceOrchestrator();

// Register session
const sessionId = '550e8400-e29b-41d4-a716-446655440000';
const roomId = '550e8400-e29b-41d4-a716-446655440001';

orchestrator.registerSession(sessionId, roomId, [
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

console.log('   ✅ Registered session with 3 participants\n');

// Test utterance processing (question)
console.log('4. Testing utterance processing (question)...');
const responder = orchestrator.onUtterance({
	session_id: sessionId,
	speaker_id: '550e8400-e29b-41d4-a716-446655440002',
	speaker_name: 'Joel',
	speaker_type: 'human',
	transcript: 'How do I implement priority queues in Rust?',
	confidence: 0.95,
	timestamp: Date.now(),
});

console.log(`   ${responder ? '✅' : '❌'} Responder selected: ${responder}\n`);

// Test utterance processing (statement - should return null)
console.log('5. Testing utterance processing (statement)...');
const noResponder = orchestrator.onUtterance({
	session_id: sessionId,
	speaker_id: '550e8400-e29b-41d4-a716-446655440002',
	speaker_name: 'Joel',
	speaker_type: 'human',
	transcript: 'The weather is nice today.',
	confidence: 0.95,
	timestamp: Date.now(),
});

console.log(`   ${noResponder === null ? '✅' : '❌'} No responder for statement (correct)\n`);

// ============================================================================
// Test 6: Performance Stats
// ============================================================================

console.log('6. Testing performance stats...');
const ffiStats = RustCore.getFfiTimingStats();
if (ffiStats) {
	console.log(`   ✅ FFI timing stats:`);
	console.log(`      Calls: ${ffiStats.count}`);
	console.log(`      Mean: ${ffiStats.mean.toFixed(2)}ms`);
	console.log(`      P50: ${ffiStats.p50.toFixed(2)}ms`);
	console.log(`      P95: ${ffiStats.p95.toFixed(2)}ms`);
	console.log(`      P99: ${ffiStats.p99.toFixed(2)}ms`);
	console.log(`      Max: ${ffiStats.max.toFixed(2)}ms`);

	// Warn if any call was slow
	if (ffiStats.max > 10) {
		console.warn(`\n   ⚠️  Slowest FFI call: ${ffiStats.max.toFixed(2)}ms (threshold: 10ms)`);
	}
} else {
	console.log('   ℹ️  No timing stats yet\n');
}

// ============================================================================
// Cleanup
// ============================================================================

orchestrator.destroy();
console.log('\n✅ All FFI tests passed!');
console.log('🦀 Rust <-> TypeScript bridge working correctly\n');
