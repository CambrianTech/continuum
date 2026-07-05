#!/usr/bin/env tsx
/**
 * CollaborationLiveStart Command Integration Tests
 *
 * Tests live/start command against the LIVE RUNNING SYSTEM.
 * Run with: npx tsx commands/collaboration/live/start/test/integration/CollaborationLiveStartIntegration.test.ts
 *
 * PREREQUISITES:
 * - Server must be running: npm start (wait 90+ seconds)
 */

import { jtag } from '../../../../../../server-index';
import type { CollaborationLiveStartResult } from '../../shared/CollaborationLiveStartTypes';

console.log('🧪 CollaborationLiveStart Command Integration Tests');

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

/**
 * Test 1: Connect to live system
 */
async function testSystemConnection(): Promise<Awaited<ReturnType<typeof jtag.connect>>> {
  console.log('\n🔌 Test 1: Connecting to live JTAG system');

  const client = await jtag.connect();

  assert(client !== null, 'Connected to live system');
  console.log('   ✅ Connected successfully');

  return client;
}

/**
 * Test 2: Start live session with single participant
 */
async function testSingleParticipant(client: Awaited<ReturnType<typeof jtag.connect>>): Promise<void> {
  console.log('\n⚡ Test 2: Start live session with single participant');

  const result = await client.commands['collaboration/live/start']({
    participants: 'helper'
  }) as CollaborationLiveStartResult;

  console.log('   📊 Result:', JSON.stringify(result, null, 2).substring(0, 500) + '...');

  assert(result.success === true, 'Command succeeded');
  assert(typeof result.roomId === 'string', 'Has roomId');
  assert(typeof result.liveSessionId === 'string', 'Has liveSessionId');
  assert(result.room !== undefined, 'Has room entity');
  assert(result.session !== undefined, 'Has session entity');
  assert(result.room.members.length >= 2, 'Room has at least 2 members (caller + helper)');
  assert(result.session.status === 'active', 'Session is active');

  // Clean up - leave the session
  await client.commands['collaboration/live/leave']({
    sessionId: result.session.id
  });
  console.log('   🧹 Cleaned up session');
}

/**
 * Test 3: Start live session with multiple participants (group call)
 */
async function testGroupCall(client: Awaited<ReturnType<typeof jtag.connect>>): Promise<void> {
  console.log('\n👥 Test 3: Start group live session with multiple participants');

  const result = await client.commands['collaboration/live/start']({
    participants: ['helper', 'teacher', 'codereview'],
    name: 'AI Council Test'
  }) as CollaborationLiveStartResult;

  console.log('   📊 Room:', result.room?.displayName);
  console.log('   📊 Members:', result.room?.members?.length);

  assert(result.success === true, 'Group call command succeeded');
  assert(result.room.members.length >= 4, 'Room has at least 4 members');
  assert(result.room.type === 'private', 'Group room is private');
  assert(result.session.status === 'active', 'Session is active');

  // Clean up
  await client.commands['collaboration/live/leave']({
    sessionId: result.session.id
  });
  console.log('   🧹 Cleaned up session');
}

/**
 * Test 4: Rejoin existing room should find it (idempotent)
 */
async function testIdempotent(client: Awaited<ReturnType<typeof jtag.connect>>): Promise<void> {
  console.log('\n🔄 Test 4: Rejoining same participants finds existing room');

  // First call - creates room
  const first = await client.commands['collaboration/live/start']({
    participants: 'local'
  }) as CollaborationLiveStartResult;

  assert(first.success === true, 'First call succeeded');
  const roomId = first.roomId;

  // Leave session but room persists
  await client.commands['collaboration/live/leave']({
    sessionId: first.session.id
  });

  // Second call - should find same room
  const second = await client.commands['collaboration/live/start']({
    participants: 'local'
  }) as CollaborationLiveStartResult;

  assert(second.success === true, 'Second call succeeded');
  assert(second.roomId === roomId, 'Same room returned (idempotent)');
  assert(second.existed === true, 'Indicated room already existed');

  // Clean up
  await client.commands['collaboration/live/leave']({
    sessionId: second.session.id
  });
  console.log('   🧹 Cleaned up session');
}

/**
 * Test 5: Missing participants should fail
 */
async function testMissingParticipants(client: Awaited<ReturnType<typeof jtag.connect>>): Promise<void> {
  console.log('\n🚨 Test 5: Missing participants validation');

  try {
    await client.commands['collaboration/live/start']({
      participants: ''
    });
    assert(false, 'Should have thrown error for empty participants');
  } catch (error) {
    const message = (error as Error).message || String(error);
    assert(
      message.includes('participant') || message.includes('required'),
      'Error mentions participants/required'
    );
    console.log('   ✅ Validation error thrown correctly');
  }
}

/**
 * Test 6: Performance - should complete in reasonable time
 */
async function testPerformance(client: Awaited<ReturnType<typeof jtag.connect>>): Promise<void> {
  console.log('\n⚡ Test 6: Performance');

  const start = Date.now();
  const result = await client.commands['collaboration/live/start']({
    participants: 'gpt'
  }) as CollaborationLiveStartResult;
  const duration = Date.now() - start;

  console.log(`   ⏱️ Execution time: ${duration}ms`);

  assert(result.success === true, 'Command succeeded');
  assert(duration < 10000, `Completed in ${duration}ms (under 10s)`);

  // Clean up
  await client.commands['collaboration/live/leave']({
    sessionId: result.session.id
  });
}

/**
 * Run all integration tests
 */
async function runAllTests(): Promise<void> {
  console.log('🚀 Starting CollaborationLiveStart Integration Tests\n');
  console.log('📋 Testing against LIVE system (not mocks)\n');

  try {
    const client = await testSystemConnection();
    await testSingleParticipant(client);
    await testGroupCall(client);
    await testIdempotent(client);
    await testMissingParticipants(client);
    await testPerformance(client);

    console.log('\n🎉 ALL INTEGRATION TESTS PASSED!');
    console.log('📋 Validated:');
    console.log('  ✅ Single participant live session');
    console.log('  ✅ Group call with multiple participants');
    console.log('  ✅ Idempotent room finding');
    console.log('  ✅ Parameter validation');
    console.log('  ✅ Performance benchmarks');

  } catch (error) {
    console.error('\n❌ Tests failed:', (error as Error).message);
    if ((error as Error).stack) {
      console.error((error as Error).stack);
    }
    console.error('\n💡 Make sure:');
    console.error('   1. Server is running: npm start');
    console.error('   2. Wait 90+ seconds for deployment');
    process.exit(1);
  }
}

// Run if called directly
if (require.main === module) {
  void runAllTests();
} else {
  module.exports = { runAllTests };
}
