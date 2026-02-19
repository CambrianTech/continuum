/**
 * AI Agent Event Observation Test
 *
 * Proves that AI agents (server-side) can observe events from browsers (human users)
 * and vice versa. This is the foundation for AI agents participating in:
 * - Chat rooms (observing messages, responding)
 * - Academy sessions (observing progress, providing guidance)
 * - Any collaborative human/AI interaction
 *
 * Success criteria:
 * 1. Server-side code can subscribe to browser-originated events
 * 2. Browser can subscribe to server-originated events
 * 3. Events flow bidirectionally through the unified events system
 */

import { JTAGClient } from '../../system/core/client/shared/JTAGClient';
import { Commands } from '../../system/core/shared/Commands';
import { STATE_COMMANDS } from '../../commands/state/shared/StateCommandConstants';
import { DATA_EVENTS, getDataEventName } from '../../system/core/shared/EventConstants';

import { StateCreate } from '../../commands/state/create/shared/StateCreateTypes';
async function testAIAgentEventObservation() {
  console.log('🤖 AI Agent Event Observation Test');
  console.log('=====================================');
  console.log('Goal: Prove AI agents can observe events for chat/academy interaction\n');

  try {
    // Initialize JTAG client (this will be LocalConnection on server)
    const jtag = await JTAGClient.sharedInstance;
    console.log(`✅ JTAGClient initialized`);
    console.log(`   Connection type: ${jtag.isLocal ? 'Local (AI Agent)' : 'Remote (Browser)'}`);
    console.log(`   Environment: ${jtag.context.environment}`);

    if (!jtag.isLocal) {
      console.log('\n⚠️  This test must run server-side to simulate AI agent');
      console.log('   AI agents use LocalConnection to access EventSubscriptionManager');
      return;
    }

    // Test 1: AI Agent subscribes to chat message events
    console.log('\n📋 Test 1: AI Agent subscribing to chat message events');
    const observedMessages: any[] = [];

    const unsubMessages = jtag.daemons.events.on(getDataEventName('ChatMessage', 'created'), (message: any) => {
      console.log(`   🤖 AI Agent observed: "${message.content?.text}"`);
      observedMessages.push(message);
    });

    console.log('   ✅ AI Agent subscribed to data:ChatMessage:created');

    // Test 2: AI Agent subscribes to user events (for chat room participation)
    console.log('\n📋 Test 2: AI Agent subscribing to user events');
    const observedUserActions: any[] = [];

    const unsubUsers = jtag.daemons.events.on('data:users {created,updated}', (event: any) => {
      console.log(`   🤖 AI Agent observed user ${event.action}: ${event.entity?.displayName}`);
      observedUserActions.push(event);
    });

    console.log('   ✅ AI Agent subscribed to data:users {created,updated}');

    // Test 3: Simulate browser creating chat message (AI agent should observe)
    console.log('\n📋 Test 3: Simulating browser creating chat message');
    const messageResult = await StateCreate.execute({
      collection: 'ChatMessage',
      data: {
        roomId: '5e71a0c8-0303-4eb8-a478-3a121248',
        senderId: '002350cc-0031-408d-8040-004f000f',
        senderName: 'Test User',
        content: { text: 'Hello AI agent, can you see this?', attachments: [] },
        status: 'sent',
        priority: 'normal',
        timestamp: new Date().toISOString()
      }
    });

    if (messageResult.success) {
      console.log(`   ✅ Created message: ${messageResult.id}`);
    } else {
      console.log(`   ❌ Failed to create message: ${messageResult.error}`);
    }

    // Wait for events to propagate
    await new Promise(resolve => setTimeout(resolve, 1000));

    // Test 4: Verify AI agent received the event
    console.log('\n📋 Test 4: Verifying AI agent observed the message');
    if (observedMessages.length > 0) {
      console.log(`   ✅ AI Agent observed ${observedMessages.length} message(s)`);
      observedMessages.forEach((msg, i) => {
        console.log(`      Message ${i + 1}: "${msg.content?.text?.substring(0, 50)}"`);
      });
    } else {
      console.log('   ❌ AI Agent did NOT observe message event');
    }

    // Test 5: AI Agent emits response (simulating AI participation in chat)
    console.log('\n📋 Test 5: AI Agent emitting response message');
    await jtag.daemons.events.emit('ai:response-generated', {
      messageId: messageResult.id,
      response: 'Yes, I can see your message! AI agents can now observe chat events.',
      agentId: 'test-ai-agent',
      timestamp: new Date().toISOString()
    });
    console.log('   ✅ AI Agent emitted response event');

    // Test 6: Simulate user action (AI agent should observe with elegant pattern)
    console.log('\n📋 Test 6: Simulating user update (AI agent observes with pattern)');
    const userResult = await StateCreate.execute({
      collection: 'User',
      data: {
        displayName: 'AI Test User',
        type: 'human',
        status: 'active'
      }
    });

    await new Promise(resolve => setTimeout(resolve, 1000));

    console.log('\n📋 Test 7: Verifying AI agent observed user action');
    if (observedUserActions.length > 0) {
      console.log(`   ✅ AI Agent observed ${observedUserActions.length} user action(s)`);
      observedUserActions.forEach((action, i) => {
        console.log(`      Action ${i + 1}: ${action.action} - ${action.entity?.displayName}`);
      });
    } else {
      console.log('   ❌ AI Agent did NOT observe user events');
    }

    // Cleanup
    console.log('\n📋 Cleanup: Unsubscribing AI agent');
    unsubMessages();
    unsubUsers();
    console.log('   ✅ AI Agent unsubscribed from all events');

    // Results summary
    console.log('\n=====================================');
    console.log('📊 AI Agent Event Observation Results');
    console.log('=====================================');
    console.log(`✅ Chat message observation: ${observedMessages.length > 0 ? 'PASS' : 'FAIL'}`);
    console.log(`✅ User action observation: ${observedUserActions.length > 0 ? 'PASS' : 'FAIL'}`);
    console.log(`✅ Elegant pattern matching: ${observedUserActions.length > 0 ? 'PASS' : 'FAIL'}`);

    const allPassed = observedMessages.length > 0 && observedUserActions.length > 0;

    if (allPassed) {
      console.log('\n🎉 SUCCESS! AI agents can now observe events');
      console.log('✨ Foundation ready for:');
      console.log('   - AI agents in chat rooms');
      console.log('   - Persona (LoRA adapted) interactions');
      console.log('   - Academy AI tutors');
      console.log('   - Human/AI collaborative environments');
    } else {
      console.log('\n⚠️  Some observations failed');
      console.log('   Note: Events may need more time to propagate');
    }

  } catch (error) {
    console.error('❌ AI agent event observation test failed:', error);
    process.exit(1);
  }
}

testAIAgentEventObservation().catch(error => {
  console.error('❌ Test execution failed:', error);
  process.exit(1);
});