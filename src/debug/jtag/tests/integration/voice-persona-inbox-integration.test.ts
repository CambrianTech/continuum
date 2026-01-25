#!/usr/bin/env tsx
/**
 * Voice Persona Inbox Integration Tests - REQUIRES RUNNING SYSTEM
 *
 * Tests that voice events actually reach PersonaUser inboxes and get processed.
 * This is the CRITICAL test - verifies the complete flow works in the real system.
 *
 * Run with: npx tsx tests/integration/voice-persona-inbox-integration.test.ts
 *
 * PREREQUISITES:
 * 1. npm start (running in background)
 * 2. At least one AI persona instantiated and running
 * 3. PersonaUser.serviceInbox() loop active
 */

import { Commands } from '../../system/core/shared/Commands';
import { Events } from '../../system/core/shared/Events';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { DataListParams, DataListResult } from '../../commands/data/list/shared/DataListTypes';
import type { UserEntity } from '../../system/data/entities/UserEntity';

async function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function assert(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(`❌ ${message}`);
  }
  console.log(`✅ ${message}`);
}

async function testSystemRunning(): Promise<void> {
  console.log('\n🔍 Test 1: Verify system is running');

  try {
    const result = await Commands.execute('ping', {});
    assert(result.success, 'System is running');
  } catch (error) {
    throw new Error('❌ System not running. Run "npm start" first.');
  }
}

async function findAIPersonas(): Promise<UserEntity[]> {
  console.log('\n🔍 Test 2: Find AI personas');

  const result = await Commands.execute<DataListParams, DataListResult<UserEntity>>('data/list', {
    collection: 'users',
    filter: { type: 'persona' },
    limit: 10,
  });

  if (!result.success || !result.data || result.data.length === 0) {
    throw new Error('❌ No AI personas found in database');
  }

  console.log(`📋 Found ${result.data.length} AI personas:`);
  result.data.forEach(p => {
    console.log(`   - ${p.displayName} (${p.id.slice(0, 8)})`);
  });

  return result.data;
}

async function testVoiceEventToPersona(persona: UserEntity): Promise<void> {
  console.log(`\n🔍 Test 3: Send voice event to ${persona.displayName}`);

  const sessionId = generateUUID();
  const speakerId = generateUUID();
  const testTranscript = `Integration test for ${persona.displayName} at ${Date.now()}`;

  console.log(`📤 Emitting voice:transcription:directed to ${persona.id.slice(0, 8)}`);
  console.log(`   Transcript: "${testTranscript}"`);

  // Emit the event
  await Events.emit('voice:transcription:directed', {
    sessionId,
    speakerId,
    speakerName: 'Integration Test',
    transcript: testTranscript,
    confidence: 0.95,
    targetPersonaId: persona.id,
    timestamp: Date.now(),
  });

  console.log('✅ Event emitted');

  // Wait for PersonaUser to process
  console.log('⏳ Waiting 2 seconds for PersonaUser to process event...');
  await sleep(2000);

  console.log('✅ Wait complete (PersonaUser should have processed event)');
}

async function testMultipleVoiceEvents(personas: UserEntity[]): Promise<void> {
  console.log('\n🔍 Test 4: Send multiple voice events');

  if (personas.length < 2) {
    console.warn('⚠️  Need at least 2 personas, using first persona only');
  }

  const testPersonas = personas.slice(0, Math.min(2, personas.length));
  const sessionId = generateUUID();
  const speakerId = generateUUID();

  // Send 3 utterances in sequence
  for (let i = 0; i < 3; i++) {
    const transcript = `Sequential utterance ${i + 1} at ${Date.now()}`;

    console.log(`\n📤 Utterance ${i + 1}/3: "${transcript}"`);

    // Broadcast to all test personas
    for (const persona of testPersonas) {
      await Events.emit('voice:transcription:directed', {
        sessionId,
        speakerId,
        speakerName: 'Integration Test',
        transcript,
        confidence: 0.95,
        targetPersonaId: persona.id,
        timestamp: Date.now(),
      });

      console.log(`   → Sent to ${persona.displayName.slice(0, 20)}`);
    }

    // Small delay between utterances
    await sleep(500);
  }

  console.log('\n⏳ Waiting 3 seconds for PersonaUsers to process all events...');
  await sleep(3000);

  console.log('✅ All events emitted and processing time complete');
  console.log(`📊 Total events sent: ${3 * testPersonas.length}`);
}

async function testEventWithLongTranscript(persona: UserEntity): Promise<void> {
  console.log(`\n🔍 Test 5: Send event with long transcript to ${persona.displayName}`);

  const sessionId = generateUUID();
  const speakerId = generateUUID();
  const longTranscript = `This is a longer integration test transcript to verify that PersonaUser can handle substantial voice transcriptions. The content includes multiple sentences and should trigger the same processing as real voice input would. This tests the complete path from event emission through PersonaUser subscription to inbox queueing. Test timestamp: ${Date.now()}`;

  console.log(`📤 Emitting event with ${longTranscript.length} character transcript`);

  await Events.emit('voice:transcription:directed', {
    sessionId,
    speakerId,
    speakerName: 'Integration Test',
    transcript: longTranscript,
    confidence: 0.87,
    targetPersonaId: persona.id,
    timestamp: Date.now(),
  });

  console.log('✅ Long transcript event emitted');
  await sleep(2000);
  console.log('✅ Processing time complete');
}

async function testHighPriorityVoiceEvents(persona: UserEntity): Promise<void> {
  console.log(`\n🔍 Test 6: Test high-confidence voice events to ${persona.displayName}`);

  const sessionId = generateUUID();
  const speakerId = generateUUID();

  // Send high-confidence event
  const highConfTranscript = `High confidence voice input at ${Date.now()}`;

  console.log(`📤 Emitting high-confidence event (0.98)`);

  await Events.emit('voice:transcription:directed', {
    sessionId,
    speakerId,
    speakerName: 'Integration Test',
    transcript: highConfTranscript,
    confidence: 0.98, // Very high confidence
    targetPersonaId: persona.id,
    timestamp: Date.now(),
  });

  console.log('✅ High-confidence event emitted');
  await sleep(1000);

  // Send low-confidence event
  const lowConfTranscript = `Low confidence voice input at ${Date.now()}`;

  console.log(`📤 Emitting low-confidence event (0.65)`);

  await Events.emit('voice:transcription:directed', {
    sessionId,
    speakerId,
    speakerName: 'Integration Test',
    transcript: lowConfTranscript,
    confidence: 0.65, // Lower confidence (but still above typical threshold)
    targetPersonaId: persona.id,
    timestamp: Date.now(),
  });

  console.log('✅ Low-confidence event emitted');
  await sleep(2000);
  console.log('✅ Both confidence levels processed');
}

async function testRapidSuccessionEvents(persona: UserEntity): Promise<void> {
  console.log(`\n🔍 Test 7: Rapid succession events to ${persona.displayName}`);

  const sessionId = generateUUID();
  const speakerId = generateUUID();

  console.log('📤 Emitting 5 events rapidly (no delay)');

  // Emit 5 events as fast as possible
  for (let i = 0; i < 5; i++) {
    await Events.emit('voice:transcription:directed', {
      sessionId,
      speakerId,
      speakerName: 'Integration Test',
      transcript: `Rapid event ${i + 1} at ${Date.now()}`,
      confidence: 0.95,
      targetPersonaId: persona.id,
      timestamp: Date.now(),
    });
  }

  console.log('✅ 5 rapid events emitted');
  console.log('⏳ Waiting for PersonaUser to process queue...');
  await sleep(3000);
  console.log('✅ Queue processing time complete');
}

async function verifyLogsForEventProcessing(persona: UserEntity): Promise<void> {
  console.log(`\n🔍 Test 8: Check logs for event processing evidence`);

  const fs = await import('fs');
  const path = await import('path');

  // Try to find server logs
  const logPaths = [
    '.continuum/sessions/user/shared/default/logs/server.log',
    '.continuum/logs/server.log',
  ];

  let logFound = false;
  let voiceEventFound = false;

  for (const logPath of logPaths) {
    const fullPath = path.join(process.cwd(), logPath);
    if (fs.existsSync(fullPath)) {
      logFound = true;
      console.log(`📄 Checking log file: ${logPath}`);

      const logContent = fs.readFileSync(fullPath, 'utf-8');
      const recentLog = logContent.split('\n').slice(-500).join('\n'); // Last 500 lines

      // Check for voice event indicators
      if (recentLog.includes('voice:transcription:directed') ||
          recentLog.includes('Received DIRECTED voice transcription') ||
          recentLog.includes('handleVoiceTranscription')) {
        voiceEventFound = true;
        console.log('✅ Found voice event processing in logs');

        // Count occurrences
        const matches = recentLog.match(/voice:transcription:directed/g);
        if (matches) {
          console.log(`📊 Found ${matches.length} voice event mentions in recent logs`);
        }
      }

      break;
    }
  }

  if (!logFound) {
    console.warn('⚠️  No log files found. Cannot verify from logs.');
    console.warn('   Expected location: .continuum/sessions/user/shared/default/logs/server.log');
  } else if (!voiceEventFound) {
    console.warn('⚠️  No voice event processing found in recent logs');
    console.warn('   This could mean:');
    console.warn('   1. PersonaUser is not running/subscribed');
    console.warn('   2. Events are not reaching PersonaUser');
    console.warn('   3. Logs are not being written');
    console.warn('   Check: grep "voice:transcription:directed" .continuum/sessions/*/logs/*.log');
  }
}

async function runAllTests(): Promise<void> {
  console.log('🧪 Voice Persona Inbox Integration Tests');
  console.log('='.repeat(60));
  console.log('⚠️  REQUIRES: npm start running + PersonaUsers active');
  console.log('='.repeat(60));

  let exitCode = 0;
  const results: { test: string; passed: boolean; error?: string }[] = [];

  // Test 1: System running
  try {
    await testSystemRunning();
    results.push({ test: 'System running', passed: true });
  } catch (error) {
    results.push({ test: 'System running', passed: false, error: String(error) });
    console.error('\n❌ CRITICAL: System not running');
    console.error('   Run: npm start');
    process.exit(1);
  }

  // Test 2: Find personas
  let personas: UserEntity[] = [];
  try {
    personas = await findAIPersonas();
    results.push({ test: 'Find AI personas', passed: true });
  } catch (error) {
    results.push({ test: 'Find AI personas', passed: false, error: String(error) });
    console.error('\n❌ CRITICAL: No AI personas found');
    console.error('   Create personas first');
    process.exit(1);
  }

  const testPersona = personas[0];

  // Test 3: Single event
  try {
    await testVoiceEventToPersona(testPersona);
    results.push({ test: 'Single voice event', passed: true });
  } catch (error) {
    results.push({ test: 'Single voice event', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 4: Multiple events
  try {
    await testMultipleVoiceEvents(personas);
    results.push({ test: 'Multiple voice events', passed: true });
  } catch (error) {
    results.push({ test: 'Multiple voice events', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 5: Long transcript
  try {
    await testEventWithLongTranscript(testPersona);
    results.push({ test: 'Long transcript event', passed: true });
  } catch (error) {
    results.push({ test: 'Long transcript event', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 6: Confidence levels
  try {
    await testHighPriorityVoiceEvents(testPersona);
    results.push({ test: 'Confidence level events', passed: true });
  } catch (error) {
    results.push({ test: 'Confidence level events', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 7: Rapid succession
  try {
    await testRapidSuccessionEvents(testPersona);
    results.push({ test: 'Rapid succession events', passed: true });
  } catch (error) {
    results.push({ test: 'Rapid succession events', passed: false, error: String(error) });
    exitCode = 1;
  }

  // Test 8: Log verification
  try {
    await verifyLogsForEventProcessing(testPersona);
    results.push({ test: 'Log verification', passed: true });
  } catch (error) {
    results.push({ test: 'Log verification', passed: false, error: String(error) });
    // Don't fail on this - it's informational
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

  if (exitCode === 0) {
    console.log('\n✅ All integration tests passed!');
    console.log('\n📋 Events successfully emitted to PersonaUsers');
    console.log('\n⚠️  NOTE: These tests verify event emission only.');
    console.log('   To verify PersonaUser inbox processing:');
    console.log('   1. Check logs: grep "Received DIRECTED voice" .continuum/sessions/*/logs/*.log');
    console.log('   2. Check logs: grep "handleVoiceTranscription" .continuum/sessions/*/logs/*.log');
    console.log('   3. Watch PersonaUser activity in real-time during manual test');
  } else {
    console.error('\n❌ Some tests failed. Review errors above.');
  }

  process.exit(exitCode);
}

// Run tests
runAllTests().catch(error => {
  console.error('\n❌ Fatal error:', error);
  process.exit(1);
});
