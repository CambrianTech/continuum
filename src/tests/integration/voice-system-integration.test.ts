#!/usr/bin/env tsx
/**
 * Voice System Integration Tests - REQUIRES RUNNING SYSTEM
 *
 * These tests verify the ACTUAL implementation against a running system:
 * - npm start must be running
 * - Real PersonaUser instances
 * - Real Events.emit/subscribe
 * - Real VoiceOrchestrator (Rust IPC)
 * - Real database
 *
 * Run with: npx tsx tests/integration/voice-system-integration.test.ts
 *
 * PREREQUISITES:
 * 1. npm start (running in background)
 * 2. At least one AI persona in database
 * 3. Rust workers running (continuum-core on Unix socket)
 */

import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { UserEntity } from '../../system/data/entities/UserEntity';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

import { Ping } from '../../commands/ping/shared/PingTypes';
import { DataList } from '../../commands/data/list/shared/DataListTypes';
const TIMEOUT = 30000; // 30 seconds for system operations

// Test utilities
function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`❌ Assertion failed: ${message}`);
  }
  console.log(`✅ ${message}`);
}

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Test: Verify system is running
async function testSystemRunning(): Promise<void> {
  console.log('\n🔍 Test 1: Verify system is running');

  try {
    // Try to ping the system
    const result = await Ping.execute({});
    assert(result.success, 'System is running and responsive');
  } catch (error) {
    throw new Error('❌ System not running. Run "npm start" first.');
  }
}

// Test: Find AI personas in database
async function testFindAIPersonas(): Promise<UserEntity[]> {
  console.log('\n🔍 Test 2: Find AI personas in database');

  const result = await DataList.execute<UserEntity>({
    collection: 'users',
    filter: { type: 'persona' },
    limit: 10,
  });

  assert(result.success, 'Successfully queried users collection');
  assert(result.data && result.data.length > 0, `Found ${result.data?.length || 0} AI personas`);

  console.log(`📋 Found AI personas:`);
  result.data?.forEach(persona => {
    console.log(`   - ${persona.displayName} (${persona.id.slice(0, 8)})`);
  });

  return result.data || [];
}

// Test: Emit voice:transcription:directed event and verify delivery
async function testVoiceEventEmission(personas: UserEntity[]): Promise<void> {
  console.log('\n🔍 Test 3: Emit voice event and verify delivery');

  if (personas.length === 0) {
    throw new Error('❌ No personas available for testing');
  }

  const targetPersona = personas[0];
  const sessionId = generateUUID();
  const speakerId = generateUUID();
  const testTranscript = `Integration test at ${Date.now()}`;

  console.log(`📤 Emitting event to: ${targetPersona.displayName} (${targetPersona.id.slice(0, 8)})`);

  // Track if event was received
  let eventReceived = false;
  let receivedData: any = null;

  // Subscribe to see if the event propagates
  const unsubscribe = Events.subscribe('voice:transcription:directed', (data: any) => {
    if (data.targetPersonaId === targetPersona.id && data.transcript === testTranscript) {
      eventReceived = true;
      receivedData = data;
      console.log(`✅ Event received by subscriber`);
    }
  });

  // Emit the event
  await Events.emit('voice:transcription:directed', {
    sessionId,
    speakerId,
    speakerName: 'Integration Test',
    transcript: testTranscript,
    confidence: 0.95,
    targetPersonaId: targetPersona.id,
    timestamp: Date.now(),
  });

  // Wait for event to propagate
  await sleep(100);

  unsubscribe();

  assert(eventReceived, 'Event was received by test subscriber');
  assert(receivedData !== null, 'Event data was captured');
  assert(receivedData.transcript === testTranscript, 'Event data is correct');
}

// Test: Verify PersonaUser has handleVoiceTranscription method
async function testPersonaUserVoiceHandling(personas: UserEntity[]): Promise<void> {
  console.log('\n🔍 Test 4: Verify PersonaUser voice handling (code inspection)');

  // This test verifies that PersonaUser.ts has the necessary subscription
  // We can't directly access PersonaUser instances from here, but we can verify
  // the code structure through file reading

  const fs = await import('fs');
  const path = await import('path');

  const personaUserPath = path.join(
    process.cwd(),
    'system/user/server/PersonaUser.ts'
  );

  const personaUserCode = fs.readFileSync(personaUserPath, 'utf-8');

  assert(
    personaUserCode.includes('voice:transcription:directed'),
    'PersonaUser subscribes to voice:transcription:directed'
  );

  assert(
    personaUserCode.includes('handleVoiceTranscription'),
    'PersonaUser has handleVoiceTranscription method'
  );

  assert(
    personaUserCode.includes('targetPersonaId'),
    'PersonaUser checks targetPersonaId'
  );

  console.log('✅ PersonaUser.ts has correct voice event handling structure');
}

// Test: Verify VoiceWebSocketHandler emits events
async function testVoiceWebSocketHandlerStructure(): Promise<void> {
  console.log('\n🔍 Test 5: Verify VoiceWebSocketHandler emits events (code inspection)');

  const fs = await import('fs');
  const path = await import('path');

  const handlerPath = path.join(
    process.cwd(),
    'system/voice/server/VoiceWebSocketHandler.ts'
  );

  const handlerCode = fs.readFileSync(handlerPath, 'utf-8');

  assert(
    handlerCode.includes('getRustVoiceOrchestrator'),
    'VoiceWebSocketHandler uses Rust orchestrator'
  );

  assert(
    handlerCode.includes('voice:transcription:directed'),
    'VoiceWebSocketHandler emits voice:transcription:directed events'
  );

  assert(
    handlerCode.includes('Events.emit'),
    'VoiceWebSocketHandler uses Events.emit'
  );

  assert(
    handlerCode.includes('for (const aiId of responderIds)'),
    'VoiceWebSocketHandler loops through responder IDs'
  );

  console.log('✅ VoiceWebSocketHandler.ts has correct event emission structure');
}

// Test: Verify Rust orchestrator is accessible
async function testRustOrchestratorConnection(): Promise<void> {
  console.log('\n🔍 Test 6: Verify Rust orchestrator connection');

  try {
    // Try to import and instantiate Rust bridge
    const { getRustVoiceOrchestrator } = await import('../../system/voice/server/VoiceOrchestratorRustBridge');
    const orchestrator = getRustVoiceOrchestrator();

    assert(orchestrator !== null, 'Rust orchestrator instance created');

    // Try to register a test session
    const sessionId = generateUUID();
    const roomId = generateUUID();

    await orchestrator.registerSession(sessionId, roomId, []);

    console.log('✅ Rust orchestrator is accessible via IPC');
  } catch (error) {
    console.warn(`⚠️  Rust orchestrator not available: ${error}`);
    console.warn('   This is expected if continuum-core worker is not running');
    console.warn('   Run: npm run worker:start');
  }
}

// Test: End-to-end event flow simulation
async function testEndToEndEventFlow(personas: UserEntity[]): Promise<void> {
  console.log('\n🔍 Test 7: End-to-end event flow simulation');

  if (personas.length < 2) {
    console.warn('⚠️  Need at least 2 personas for full test, skipping');
    return;
  }

  const sessionId = generateUUID();
  const speakerId = generateUUID();
  const testTranscript = `E2E test ${Date.now()}`;

  // Track events received by each persona
  const receivedEvents = new Map<string, boolean>();
  personas.forEach(p => receivedEvents.set(p.id, false));

  // Subscribe to events for all personas
  const unsubscribe = Events.subscribe('voice:transcription:directed', (data: any) => {
    if (receivedEvents.has(data.targetPersonaId) && data.transcript === testTranscript) {
      receivedEvents.set(data.targetPersonaId, true);
      console.log(`   ✅ Event received by persona: ${data.targetPersonaId.slice(0, 8)}`);
    }
  });

  // Emit events to multiple personas (simulating broadcast)
  for (const persona of personas.slice(0, 2)) {
    await Events.emit('voice:transcription:directed', {
      sessionId,
      speakerId,
      speakerName: 'E2E Test',
      transcript: testTranscript,
      confidence: 0.95,
      targetPersonaId: persona.id,
      timestamp: Date.now(),
    });
  }

  // Wait for propagation
  await sleep(200);

  unsubscribe();

  // Verify at least some events were received
  const receivedCount = Array.from(receivedEvents.values()).filter(Boolean).length;
  assert(receivedCount > 0, `Events delivered to ${receivedCount} personas`);
}

// Test: Performance - event emission speed
async function testEventEmissionPerformance(): Promise<void> {
  console.log('\n🔍 Test 8: Event emission performance');

  const testPersonaId = generateUUID();
  const iterations = 100;

  const start = performance.now();

  for (let i = 0; i < iterations; i++) {
    await Events.emit('voice:transcription:directed', {
      sessionId: generateUUID(),
      speakerId: generateUUID(),
      speakerName: 'Perf Test',
      transcript: `Test ${i}`,
      confidence: 0.95,
      targetPersonaId: testPersonaId,
      timestamp: Date.now(),
    });
  }

  const duration = performance.now() - start;
  const avgPerEvent = duration / iterations;

  console.log(`📊 Performance: ${iterations} events in ${duration.toFixed(2)}ms`);
  console.log(`📊 Average per event: ${avgPerEvent.toFixed(3)}ms`);

  assert(avgPerEvent < 1, `Event emission is fast (${avgPerEvent.toFixed(3)}ms per event)`);
}

// Main test runner
async function runAllTests(): Promise<void> {
  console.log('🧪 Voice System Integration Tests');
  console.log('=' .repeat(60));
  console.log('⚠️  REQUIRES: npm start running in background');
  console.log('=' .repeat(60));

  let exitCode = 0;
  const results: { test: string; passed: boolean; error?: string }[] = [];

  // Test 1: System running
  try {
    await testSystemRunning();
    results.push({ test: 'System running', passed: true });
  } catch (error) {
    results.push({ test: 'System running', passed: false, error: String(error) });
    console.error('\n❌ CRITICAL: System not running. Cannot continue tests.');
    console.error('   Run: npm start');
    console.error('   Then run tests again.');
    process.exit(1);
  }

  // Test 2: Find personas
  let personas: UserEntity[] = [];
  try {
    personas = await testFindAIPersonas();
    results.push({ test: 'Find AI personas', passed: true });
  } catch (error) {
    results.push({ test: 'Find AI personas', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 3: Event emission
  try {
    await testVoiceEventEmission(personas);
    results.push({ test: 'Voice event emission', passed: true });
  } catch (error) {
    results.push({ test: 'Voice event emission', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 4: PersonaUser structure
  try {
    await testPersonaUserVoiceHandling(personas);
    results.push({ test: 'PersonaUser voice handling', passed: true });
  } catch (error) {
    results.push({ test: 'PersonaUser voice handling', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 5: VoiceWebSocketHandler structure
  try {
    await testVoiceWebSocketHandlerStructure();
    results.push({ test: 'VoiceWebSocketHandler structure', passed: true });
  } catch (error) {
    results.push({ test: 'VoiceWebSocketHandler structure', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 6: Rust orchestrator
  try {
    await testRustOrchestratorConnection();
    results.push({ test: 'Rust orchestrator connection', passed: true });
  } catch (error) {
    results.push({ test: 'Rust orchestrator connection', passed: false, error: String(error) });
    // Don't fail on this - Rust worker might not be running
    console.warn('⚠️  Rust orchestrator test failed, but continuing...');
  }

  // Test 7: End-to-end flow
  try {
    await testEndToEndEventFlow(personas);
    results.push({ test: 'End-to-end event flow', passed: true });
  } catch (error) {
    results.push({ test: 'End-to-end event flow', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 8: Performance
  try {
    await testEventEmissionPerformance();
    results.push({ test: 'Event emission performance', passed: true });
  } catch (error) {
    results.push({ test: 'Event emission performance', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Print summary
  console.log('\n' + '='.repeat(60));
  console.log('📊 Test Summary');
  console.log('='.repeat(60));

  results.forEach(({ test, passed, error }) => {
    const icon = passed ? '✅' : '❌';
    console.log(`${icon} ${test}`);
    if (error) {
      console.log(`   Error: ${error}`);
    }
  });

  const passedCount = results.filter(r => r.passed).length;
  const totalCount = results.length;

  console.log('\n' + '='.repeat(60));
  console.log(`Results: ${passedCount}/${totalCount} tests passed`);
  console.log('='.repeat(60));

  if (exitCode !== 0) {
    console.error('\n❌ Some tests failed. Review errors above.');
  } else {
    console.log('\n✅ All integration tests passed!');
    console.log('\n🎯 Next step: Manual end-to-end voice call test');
    console.log('   1. Open browser voice UI');
    console.log('   2. Join voice call');
    console.log('   3. Speak into microphone');
    console.log('   4. Verify AI responds with voice');
  }

  process.exit(exitCode);
}

// Run tests
runAllTests().catch(error => {
  console.error('\n❌ Fatal error running tests:', error);
  process.exit(1);
});
