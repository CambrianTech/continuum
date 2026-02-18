#!/usr/bin/env tsx
/**
 * TrainingDataAccumulator Unit Test
 *
 * Tests training data accumulation, feedback attachment, and batch threshold logic
 * independent of PersonaUser and genome commands.
 */

import { TrainingDataAccumulator } from '../../system/user/server/modules/TrainingDataAccumulator';
import type { InteractionCapture, FeedbackCapture } from '../../system/user/server/modules/TrainingDataAccumulator';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

async function testTrainingDataAccumulator() {
  console.log('🧪 UNIT TEST: TrainingDataAccumulator...\n');

  let passedTests = 0;
  let totalTests = 0;

  try {
    // Create test accumulator
    const personaId = generateUUID();
    const accumulator = new TrainingDataAccumulator(personaId, 'Test AI', () => {});

    // TEST 1: Capture interaction
    console.log('📝 TEST 1: Capture interaction...');
    totalTests++;
    const capture1: InteractionCapture = {
      roleId: 'student',
      domain: 'conversation',
      input: 'What is TypeScript?',
      output: 'TypeScript is a typed superset of JavaScript.'
    };

    const interactionId = await accumulator.captureInteraction(capture1);

    if (interactionId && typeof interactionId === 'string') {
      console.log(`✅ PASS: Captured interaction with ID: ${interactionId}`);
      passedTests++;
    } else {
      console.log('❌ FAIL: No interaction ID returned');
    }

    // TEST 2: Buffer size tracking
    console.log('\n📊 TEST 2: Buffer size tracking...');
    totalTests++;
    const bufferSize = accumulator.getBufferSize('conversation');

    if (bufferSize === 1) {
      console.log(`✅ PASS: Buffer size correct (${bufferSize})`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Expected buffer size 1, got ${bufferSize}`);
    }

    // TEST 3: Multiple domain buffers
    console.log('\n🗂️ TEST 3: Multiple domain buffers...');
    totalTests++;
    const codeCapture: InteractionCapture = {
      roleId: 'developer',
      domain: 'code',
      input: 'function add(a, b) { return a + b; }',
      output: 'This function adds two numbers.'
    };

    await accumulator.captureInteraction(codeCapture);

    const conversationSize = accumulator.getBufferSize('conversation');
    const codeSize = accumulator.getBufferSize('code');

    if (conversationSize === 1 && codeSize === 1) {
      console.log(`✅ PASS: Multiple domains tracked separately`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Expected 1 for each domain, got conversation=${conversationSize}, code=${codeSize}`);
    }

    // TEST 4: Feedback attachment
    console.log('\n💬 TEST 4: Feedback attachment...');
    totalTests++;
    const feedback: FeedbackCapture = {
      interactionId,
      source: 'human',
      rating: 0.9,
      comments: 'Good explanation, very clear'
    };

    await accumulator.captureFeedback(feedback);
    const examples = await accumulator.consumeTrainingData('conversation');

    if (examples[0].feedback?.source === 'human' && examples[0].feedback.rating === 0.9) {
      console.log(`✅ PASS: Feedback attached correctly`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Feedback not attached or incorrect`);
    }

    // TEST 5: Batch threshold detection
    console.log('\n🎯 TEST 5: Batch threshold detection...');
    totalTests++;
    accumulator.setBatchThreshold('test-domain', 10); // MIN_BATCH_SIZE is 10

    // Add 9 examples (not ready)
    for (let i = 0; i < 9; i++) {
      await accumulator.captureInteraction({ ...capture1, domain: 'test-domain' });
    }

    if (!accumulator.shouldMicroTune('test-domain')) {
      console.log(`✅ PASS: Not ready at 9/10 threshold`);
    } else {
      console.log(`❌ FAIL: Incorrectly marked ready at 9/10 threshold`);
      totalTests--;
    }

    // Add 10th example (now ready)
    await accumulator.captureInteraction({ ...capture1, domain: 'test-domain' });

    if (accumulator.shouldMicroTune('test-domain')) {
      console.log(`✅ PASS: Ready at 10/10 threshold`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Not ready at 10/10 threshold`);
    }

    // TEST 6: Consume and clear buffer
    console.log('\n🔄 TEST 6: Consume and clear buffer...');
    totalTests++;
    const consumedExamples = await accumulator.consumeTrainingData('test-domain');
    const newBufferSize = accumulator.getBufferSize('test-domain');

    if (consumedExamples.length === 10 && newBufferSize === 0) {
      console.log(`✅ PASS: Consumed 10 examples, buffer cleared`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Expected 10 examples and empty buffer, got ${consumedExamples.length} examples, size ${newBufferSize}`);
    }

    // TEST 7: Get all domains
    console.log('\n📋 TEST 7: Get all domains...');
    totalTests++;
    await accumulator.captureInteraction({ ...capture1, domain: 'domain1' });
    await accumulator.captureInteraction({ ...capture1, domain: 'domain2' });

    const domains = accumulator.getDomains();

    if (domains.length === 3 && domains.includes('domain1') && domains.includes('domain2') && domains.includes('code')) {
      console.log(`✅ PASS: All domains returned: ${domains.join(', ')}`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Expected 3 domains (code, domain1, domain2), got: ${domains.join(', ')}`);
    }

    // TEST 8: Get stats
    console.log('\n📊 TEST 8: Get stats...');
    totalTests++;
    const stats = accumulator.getStats();

    if (stats['code']?.count === 1 && stats['domain1']?.count === 1 && stats['domain2']?.count === 1) {
      console.log(`✅ PASS: Stats accurate across all domains`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Stats incorrect`);
      console.log('   Expected: code=1, domain1=1, domain2=1');
      console.log('   Got:', JSON.stringify(stats, null, 2));
    }

    // TEST 9: Expected output support
    console.log('\n🎓 TEST 9: Expected output support...');
    totalTests++;
    const teachingCapture: InteractionCapture = {
      roleId: 'student',
      domain: 'teaching',
      input: 'What is 2+2?',
      output: 'The answer is 5',
      expectedOutput: 'The answer is 4'
    };

    await accumulator.captureInteraction(teachingCapture);
    const teachingExamples = await accumulator.consumeTrainingData('teaching');

    if (teachingExamples[0].expectedOutput === 'The answer is 4') {
      console.log(`✅ PASS: Expected output stored for correction learning`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Expected output not stored`);
    }

    // TEST 10: Clear all buffers
    console.log('\n🧹 TEST 10: Clear all buffers...');
    totalTests++;
    accumulator.clearAll();
    const allDomains = accumulator.getDomains();

    if (allDomains.length === 0) {
      console.log(`✅ PASS: All buffers cleared`);
      passedTests++;
    } else {
      console.log(`❌ FAIL: Buffers not cleared, still have: ${allDomains.join(', ')}`);
    }

    // RESULTS
    console.log('\n' + '='.repeat(70));
    console.log(`🎯 TEST RESULTS: ${passedTests}/${totalTests} tests passed`);
    console.log('='.repeat(70));

    if (passedTests === totalTests) {
      console.log('✅ ALL TESTS PASSED');
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
testTrainingDataAccumulator()
  .then(success => {
    process.exit(success ? 0 : 1);
  })
  .catch(error => {
    console.error('❌ FATAL TEST ERROR:', error);
    process.exit(1);
  });
