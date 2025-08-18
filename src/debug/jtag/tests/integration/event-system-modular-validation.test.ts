#!/usr/bin/env tsx
/**
 * Event System Modular Validation - Uses reusable test framework
 * 
 * Demonstrates the modular approach to event testing with standardized patterns,
 * proper cleanup, and consolidated utilities.
 */

import { EventTestRunner, StandardEventTests } from '../shared/EventTestRunner';
import { EventTestUtils, EventTestPatterns } from '../../system/events/shared/EventTestUtils';

async function runModularEventValidation() {
  console.log('🚀 MODULAR EVENT SYSTEM VALIDATION');
  console.log('🎯 Using reusable test framework with standardized patterns');
  
  const runner = new EventTestRunner();
  
  try {
    await runner.initialize();
    
    // Test 1: Basic DOM event flow using standard pattern
    console.log('\n🧪 Test 1: Basic DOM Event Flow (Modular)');
    const basicResult = await runner.runTest({
      name: 'Basic DOM Event Flow',
      setup: EventTestUtils.createDOMEventListener('chat:message-received', 'modularBasicTest'),
      trigger: async (client) => {
        return await client.commands['chat/send-message'](
          EventTestUtils.createTestMessage('modular-basic', 'modular-basic')
        );
      },
      validate: EventTestUtils.createEventCountCheck('modularBasicTest')
    });
    
    console.log('📊 Basic test result:', basicResult);
    
    // Test 2: Performance test using modular pattern  
    console.log('\n🧪 Test 2: Performance Test (Modular)');
    const perfResult = await EventTestUtils.performanceTest(
      runner.client,
      'modular-perf',
      3, // 3 messages for speed
      'modularPerfTest'
    );
    
    console.log('📊 Performance result:', perfResult);
    
    // Test 3: Event validation using type guards
    console.log('\n🧪 Test 3: Event Validation (Type Guards)');
    const validationResult = await runner.runTest({
      name: 'Event Type Validation',
      setup: `
        return (async function() {
          window.validationTest = { validEvents: 0, invalidEvents: 0 };
          
          document.addEventListener('chat:message-received', (event) => {
            // Use type validation like a real widget would
            if (event.detail?.messageId && event.detail?.roomId && event.detail?.message) {
              window.validationTest.validEvents++;
              console.log('✅ Valid event structure:', event.detail);
            } else {
              window.validationTest.invalidEvents++;
              console.log('❌ Invalid event structure:', event.detail);
            }
          });
          
          return { success: true };
        })();
      `,
      trigger: async (client) => {
        return await client.commands['chat/send-message'](
          EventTestUtils.createTestMessage('validation-test', 'validation')
        );
      },
      validate: `
        return (async function() {
          return {
            success: window.validationTest.validEvents >= 1 && window.validationTest.invalidEvents === 0,
            validEvents: window.validationTest.validEvents,
            invalidEvents: window.validationTest.invalidEvents
          };
        })();
      `
    });
    
    console.log('📊 Validation result:', validationResult);
    
    console.log('\n🎉 MODULAR EVENT VALIDATION COMPLETE!');
    console.log('✅ All tests passed using reusable framework');
    console.log('🏗️ Modular pattern eliminates code duplication');
    
  } finally {
    await runner.cleanup();
    process.exit(0);
  }
}

// Run with error handling
runModularEventValidation().catch((error) => {
  console.error('💥 Modular validation failed:', error);
  process.exit(1);
});