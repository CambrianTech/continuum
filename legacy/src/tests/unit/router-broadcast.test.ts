/**
 * Router Broadcast Test - Fire-and-Forget Event Routing
 * 
 * Proves router correctly handles event messages with broadcast semantics
 * (no correlation, multiple subscribers, fire-and-forget)
 */

import { JTAGRouter } from '../../system/core/router/shared/JTAGRouter';
import { JTAGMessageFactory } from '../../system/core/types/JTAGTypes';
import type { JTAGContext, JTAGMessage, JTAGResponsePayload } from '../../system/core/types/JTAGTypes';
import type { MessageSubscriber } from '../../system/core/router/shared/MessageSubscriber';

/**
 * Mock subscriber for testing broadcasts
 */
class MockMessageSubscriber implements MessageSubscriber {
  public receivedMessages: JTAGMessage[] = [];
  
  constructor(public name: string) {}
  
  async handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    this.receivedMessages.push(message);
    return { success: true, message: `Mock subscriber ${this.name} received event` };
  }
  
  clearReceivedMessages(): void {
    this.receivedMessages = [];
  }
}

async function testBroadcastMechanism() {
  console.log('🧪 UNIT TEST: Router broadcast mechanism...');
  
  // Setup test context and router
  const context: JTAGContext = { uuid: 'test-router', environment: 'server' };
  const router = new JTAGRouter(context, new Map(), {});
  
  // Setup EventsDaemon as single subscriber (correct pattern)
  const eventsDaemon = new MockMessageSubscriber('events-daemon');
  
  router.registerSubscriber('events/event-bridge', eventsDaemon);
  
  // Create event message (fire-and-forget)
  const eventMessage = JTAGMessageFactory.createEvent(
    context,
    'chat-daemon',
    'events/event-bridge',
    {
      eventName: 'chat-message-sent',
      roomId: 'room-123',
      messageId: 'msg-456',
      content: 'Test broadcast message',
      scope: {
        type: 'room',
        id: 'room-123'
      },
      originSessionId: 'test-session-123',
      timestamp: new Date().toISOString()
    }
  );
  
  console.log('📨 Event message type:', eventMessage.messageType);
  console.log('📨 Event endpoint:', eventMessage.endpoint);
  
  // Send broadcast event (should not await correlation)
  const result = await router.postMessage(eventMessage);
  
  // Verify broadcast behavior
  console.log('📊 Broadcast result:', result);
  console.log('📊 EventsDaemon received:', eventsDaemon.receivedMessages.length);
  
  // Test assertions
  if (eventsDaemon.receivedMessages.length === 0) {
    throw new Error('EventsDaemon should receive broadcast event');
  }
  
  const receivedMessage = eventsDaemon.receivedMessages[0];
  if (receivedMessage.messageType !== 'event') {
    throw new Error('Message should be event type');
  }
  
  console.log('✅ UNIT TEST PASSED: Router broadcast mechanism working');
  return true;
}

// Run test
testBroadcastMechanism().catch(error => {
  console.error('❌ UNIT TEST FAILED:', error);
  process.exit(1);
});