/**
 * Unified Events System Test
 *
 * Verifies that AI agents (server) and human users (browser) receive the same events
 * through the same elegant API.
 */

import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { Commands } from '../../system/core/shared/Commands';
import { STATE_COMMANDS } from '../../commands/state/shared/StateCommandConstants';
import { DATA_EVENTS } from '../../system/core/shared/EventConstants';

import { StateCreate } from '../../commands/state/create/shared/StateCreateTypes';
async function testUnifiedEvents() {
  console.log('🎯 Testing Unified Events System');
  console.log('=====================================\n');

  try {
    // Get JTAG client instance
    const jtag = await JTAGClient.sharedInstance;
    console.log('✅ JTAGClient initialized');
    console.log(`   Connection type: ${jtag.isLocal ? 'Local' : 'Remote'}`);

    // Test 1: Subscribe to exact event
    console.log('\n📋 Test 1: Exact event subscription');
    let exactEventReceived = false;
    let exactEventData: any = null;

    const unsubExact = jtag.daemons.events.on(DATA_EVENTS.USERS.CREATED, (user: any) => {
      console.log('   ✅ Received exact event:', user);
      exactEventReceived = true;
      exactEventData = user;
    });

    console.log(`   ✅ Subscribed to ${DATA_EVENTS.USERS.CREATED}`);

    // Test 2: Subscribe to elegant pattern
    console.log('\n📋 Test 2: Elegant pattern subscription');
    const patternEvents: any[] = [];

    const unsubPattern = jtag.daemons.events.on('data:users {created,updated}', (event: any) => {
      console.log('   ✅ Received pattern event:', event.action);
      patternEvents.push(event);
    });

    console.log('   ✅ Subscribed to data:users {created,updated}');

    // Test 3: Create a user to trigger events
    console.log('\n📋 Test 3: Creating user to trigger events');
    const createResult = await StateCreate.execute({
      collection: 'User',
      data: {
        displayName: 'Test User for Unified Events',
        type: 'human',
        status: 'active'
      }
    });

    if (createResult.success) {
      console.log(`   ✅ Created user: ${createResult.id}`);
    } else {
      console.log(`   ❌ Failed to create user: ${createResult.error}`);
    }

    // Wait for events to propagate
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Test 4: Verify exact subscription received event
    console.log('\n📋 Test 4: Verifying exact subscription');
    if (exactEventReceived) {
      console.log('   ✅ Exact subscription received event');
      console.log(`   User: ${exactEventData?.displayName}`);
    } else {
      console.log('   ⚠️  Exact subscription did not receive event');
    }

    // Test 5: Verify pattern subscription received event
    console.log('\n📋 Test 5: Verifying pattern subscription');
    if (patternEvents.length > 0) {
      console.log(`   ✅ Pattern subscription received ${patternEvents.length} events`);
      patternEvents.forEach((event, i) => {
        console.log(`   Event ${i + 1}: ${event.action} - ${event.entity?.displayName}`);
      });
    } else {
      console.log('   ⚠️  Pattern subscription did not receive events');
    }

    // Cleanup subscriptions
    console.log('\n📋 Cleanup: Unsubscribing');
    unsubExact();
    unsubPattern();
    console.log('   ✅ Unsubscribed from all events');

    // Summary
    console.log('\n=====================================');
    console.log('📊 Unified Events System Test Summary');
    console.log('=====================================');
    console.log(`✅ Exact subscription: ${exactEventReceived ? 'PASS' : 'FAIL'}`);
    console.log(`✅ Pattern subscription: ${patternEvents.length > 0 ? 'PASS' : 'FAIL'}`);

    const allPassed = exactEventReceived && patternEvents.length > 0;

    if (allPassed) {
      console.log('\n🎉 ALL TESTS PASSED!');
      console.log('✨ AI agents and human users can now receive the same events');
    } else {
      console.log('\n⚠️  Some tests did not pass - events may need more time to propagate');
    }

  } catch (error) {
    console.error('❌ Unified events test failed:', error);
    process.exit(1);
  }
}

testUnifiedEvents().catch(error => {
  console.error('❌ Test execution failed:', error);
  process.exit(1);
});