#!/usr/bin/env tsx
/**
 * PersonaUser Training Integration Test
 *
 * Tests TrainingDataAccumulator integration with PersonaUser and genome commands.
 * Verifies end-to-end workflow: capture interaction → attach feedback → consume for training.
 */

import { PersonaUser } from '../../system/user/server/PersonaUser';
import { UserEntity } from '../../system/data/entities/UserEntity';
import { UserStateEntity } from '../../system/data/entities/UserStateEntity';
import { MemoryStateBackend } from '../../system/user/storage/MemoryStateBackend';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { InteractionCapture, FeedbackCapture } from '../../system/user/server/modules/TrainingDataAccumulator';

async function testPersonaTrainingIntegration() {
  console.log('🧪 INTEGRATION TEST: PersonaUser + TrainingDataAccumulator...\n');

  let passedTests = 0;
  let totalTests = 0;

  try {
    // Create test PersonaUser
    console.log('🤖 Creating test PersonaUser...');
    const personaId = generateUUID();

    const entity = new UserEntity();
    entity.id = personaId;
    entity.uniqueId = 'test-student-ai';
    entity.type = 'persona';
    entity.displayName = 'Test Student AI';
    entity.status = 'online';
    entity.lastActiveAt = new Date();
    entity.capabilities = {
      canSendMessages: true,
      canReceiveMessages: true,
      canCreateRooms: false,
      canInviteOthers: false,
      canModerate: false,
      autoResponds: true,
      providesContext: false,
      canTrain: true,
      canAccessPersonas: false
    };
    entity.sessionsActive = [];

    const state = new UserStateEntity();
    state.id = generateUUID();
    state.userId = personaId;
    state.currentRoomId = generateUUID();
    state.uiPreferences = { theme: 'dark', fontSize: 'medium' };

    const storage = new MemoryStateBackend();

    const personaUser = new PersonaUser(entity, state, storage);

    console.log('✅ PersonaUser created\n');

    // TEST 1: TrainingAccumulator initialized
    console.log('📋 TEST 1: TrainingAccumulator initialized...');
    totalTests++;

    if (personaUser.trainingAccumulator) {
      console.log('✅ PASS: TrainingAccumulator exists on PersonaUser');
      passedTests++;
    } else {
      console.log('❌ FAIL: TrainingAccumulator not initialized');
    }

    // TEST 2: Capture interaction via accumulator
    console.log('\n📝 TEST 2: Capture interaction via accumulator...');
    totalTests++;

    const capture: InteractionCapture = {
      roleId: 'student',
      personaId,
      domain: 'conversation',
      input: 'Explain dependency injection',
      output: 'Dependency injection is a design pattern where...'
    };

    const interactionId = await personaUser.trainingAccumulator.captureInteraction(capture);

    if (interactionId && personaUser.trainingAccumulator.getBufferSize('conversation') === 1) {
      console.log('✅ PASS: Interaction captured successfully');
      passedTests++;
    } else {
      console.log('❌ FAIL: Interaction not captured');
    }

    // TEST 3: Attach feedback
    console.log('\n💬 TEST 3: Attach feedback...');
    totalTests++;

    const feedback: FeedbackCapture = {
      interactionId,
      source: 'ai',
      rating: 0.85,
      comments: 'Good explanation but could use more examples'
    };

    await personaUser.trainingAccumulator.captureFeedback(feedback);
    const examples = await personaUser.trainingAccumulator.consumeTrainingData('conversation');

    if (examples[0].feedback?.source === 'ai' && examples[0].feedback.rating === 0.85) {
      console.log('✅ PASS: Feedback attached correctly');
      passedTests++;
    } else {
      console.log('❌ FAIL: Feedback not attached');
    }

    // TEST 4: Multiple domains accumulation
    console.log('\n🗂️ TEST 4: Multiple domains accumulation...');
    totalTests++;

    await personaUser.trainingAccumulator.captureInteraction({
      roleId: 'developer',
      personaId,
      domain: 'code',
      input: 'function fibonacci(n) {...}',
      output: 'This implements the Fibonacci sequence'
    });

    await personaUser.trainingAccumulator.captureInteraction({
      roleId: 'reviewer',
      personaId,
      domain: 'code-review',
      input: 'Review this TypeScript code',
      output: 'The code is well-structured but...'
    });

    const domains = personaUser.trainingAccumulator.getDomains();

    if (domains.length === 2 && domains.includes('code') && domains.includes('code-review')) {
      console.log('✅ PASS: Multiple domains tracked');
      passedTests++;
    } else {
      console.log(`❌ FAIL: Expected 2 domains (code, code-review), got: ${domains.join(', ')}`);
    }

    // TEST 5: Batch threshold and readiness
    console.log('\n🎯 TEST 5: Batch threshold and readiness...');
    totalTests++;

    personaUser.trainingAccumulator.setBatchThreshold('test-learning', 10);

    for (let i = 0; i < 10; i++) {
      await personaUser.trainingAccumulator.captureInteraction({
        roleId: 'learner',
        personaId,
        domain: 'test-learning',
        input: `Test input ${i}`,
        output: `Test output ${i}`
      });
    }

    if (personaUser.trainingAccumulator.shouldMicroTune('test-learning')) {
      console.log('✅ PASS: Ready for training at threshold');
      passedTests++;
    } else {
      console.log('❌ FAIL: Not ready despite reaching threshold');
    }

    // TEST 6: Stats monitoring
    console.log('\n📊 TEST 6: Stats monitoring...');
    totalTests++;

    const stats = personaUser.trainingAccumulator.getStats();

    if (stats['code']?.count === 1 &&
        stats['code-review']?.count === 1 &&
        stats['test-learning']?.count === 10 &&
        stats['test-learning']?.ready === true) {
      console.log('✅ PASS: Stats accurate across all domains');
      passedTests++;
    } else {
      console.log('❌ FAIL: Stats incorrect');
      console.log('   Got:', JSON.stringify(stats, null, 2));
    }

    // TEST 7: Consume training data for specific domain
    console.log('\n🔄 TEST 7: Consume training data...');
    totalTests++;

    const trainingExamples = await personaUser.trainingAccumulator.consumeTrainingData('test-learning');
    const newBufferSize = personaUser.trainingAccumulator.getBufferSize('test-learning');

    if (trainingExamples.length === 10 && newBufferSize === 0) {
      console.log('✅ PASS: Consumed 10 examples, buffer cleared');
      passedTests++;
    } else {
      console.log(`❌ FAIL: Expected 10 examples and empty buffer, got ${trainingExamples.length} examples, buffer size ${newBufferSize}`);
    }

    // TEST 8: Training example structure
    console.log('\n📦 TEST 8: Training example structure...');
    totalTests++;

    const example = trainingExamples[0];

    if (example.id && example.domain === 'test-learning' &&
        example.roleId === 'learner' && example.personaId === personaId &&
        example.input && example.output && example.timestamp) {
      console.log('✅ PASS: Training example has all required fields');
      passedTests++;
    } else {
      console.log('❌ FAIL: Training example missing fields');
      console.log('   Got:', JSON.stringify(example, null, 2));
    }

    // TEST 9: Clear all for cleanup
    console.log('\n🧹 TEST 9: Clear all buffers...');
    totalTests++;

    personaUser.trainingAccumulator.clearAll();
    const finalDomains = personaUser.trainingAccumulator.getDomains();

    if (finalDomains.length === 0) {
      console.log('✅ PASS: All buffers cleared');
      passedTests++;
    } else {
      console.log(`❌ FAIL: Buffers not cleared: ${finalDomains.join(', ')}`);
    }

    // TEST 10: Accumulator persists with PersonaUser
    console.log('\n🔗 TEST 10: Accumulator persists with PersonaUser...');
    totalTests++;

    await personaUser.trainingAccumulator.captureInteraction({
      roleId: 'test',
      personaId,
      domain: 'persistence-test',
      input: 'test',
      output: 'test'
    });

    // Verify it's still the same accumulator instance
    const persistenceCheck = personaUser.trainingAccumulator.getBufferSize('persistence-test');

    if (persistenceCheck === 1) {
      console.log('✅ PASS: Accumulator persists with PersonaUser instance');
      passedTests++;
    } else {
      console.log('❌ FAIL: Accumulator state not persisted');
    }

    // RESULTS
    console.log('\n' + '='.repeat(70));
    console.log(`🎯 TEST RESULTS: ${passedTests}/${totalTests} tests passed`);
    console.log('='.repeat(70));

    if (passedTests === totalTests) {
      console.log('✅ ALL INTEGRATION TESTS PASSED');
      return true;
    } else {
      console.log(`❌ SOME TESTS FAILED (${totalTests - passedTests} failures)`);
      return false;
    }

  } catch (error) {
    console.error('❌ TEST ERROR:', error);
    return false;
  }
}

// Run tests
testPersonaTrainingIntegration()
  .then(success => {
    process.exit(success ? 0 : 1);
  })
  .catch(error => {
    console.error('❌ FATAL TEST ERROR:', error);
    process.exit(1);
  });
