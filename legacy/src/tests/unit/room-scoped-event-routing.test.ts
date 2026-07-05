/**
 * Room-Scoped Event Routing Test
 * 
 * Tests path-based event routing for room isolation using existing router hierarchy.
 * Chat widgets subscribe to specific room paths, events broadcast to those paths.
 */

import { JTAGRouter } from '../../system/core/router/shared/JTAGRouter';
import { JTAGMessageFactory } from '../../system/core/types/JTAGTypes';
import type { JTAGContext, JTAGMessage, JTAGResponsePayload } from '../../system/core/types/JTAGTypes';
import type { MessageSubscriber } from '../../system/core/router/shared/JTAGRouter';

/**
 * Mock room subscriber for testing room-scoped events
 */
class MockRoomSubscriber implements MessageSubscriber {
  public receivedMessages: JTAGMessage[] = [];
  
  constructor(public roomId: string) {}
  
  async handleMessage(message: JTAGMessage): Promise<JTAGResponsePayload> {
    this.receivedMessages.push(message);
    return { success: true, message: `Room ${this.roomId} received event` };
  }
  
  get endpoint(): string {
    return `events/chat/${this.roomId}/message-sent`;
  }
  
  get uuid(): string {
    return `room-subscriber-${this.roomId}`;
  }
}

async function testRoomScopedEventRouting() {
  console.log('🧪 UNIT TEST: Room-scoped event routing via path subscription...');
  
  // Setup test context and router
  const context: JTAGContext = { uuid: 'test-router', environment: 'server' };
  const router = new JTAGRouter(context, new Map(), {});
  
  // Setup room-specific subscribers (widgets subscribe to specific room paths)
  const room123Subscriber = new MockRoomSubscriber('room-123');
  const room456Subscriber = new MockRoomSubscriber('room-456');
  
  // Register subscribers at exact event paths (precise targeting)
  router.registerSubscriber(`events/chat/room-123/message-sent`, room123Subscriber);
  router.registerSubscriber(`events/chat/room-456/message-sent`, room456Subscriber);
  
  console.log('📋 Registered exact event subscribers:');
  console.log('  - events/chat/room-123/message-sent');
  console.log('  - events/chat/room-456/message-sent');
  
  // Create event for room-123 (should only reach room-123 subscriber)
  const room123Event = JTAGMessageFactory.createEvent(
    context,
    'chat-daemon',
    'events/chat/room-123/message-sent',
    {
      eventName: 'message-sent',
      messageId: 'msg-123',
      content: 'Hello room 123',
      senderName: 'TestUser',
      timestamp: new Date().toISOString()
    }
  );
  
  // Create event for room-456 (should only reach room-456 subscriber)  
  const room456Event = JTAGMessageFactory.createEvent(
    context,
    'chat-daemon',
    'events/chat/room-456/message-sent',
    {
      eventName: 'message-sent', 
      messageId: 'msg-456',
      content: 'Hello room 456',
      senderName: 'TestUser',
      timestamp: new Date().toISOString()
    }
  );
  
  console.log('📨 Broadcasting room-scoped events...');
  
  // Broadcast events (router should route to correct room subscribers)
  const result123 = await router.postMessage(room123Event);
  const result456 = await router.postMessage(room456Event);
  
  console.log('📊 Room 123 result:', result123);
  console.log('📊 Room 456 result:', result456);
  console.log('📊 Room 123 subscriber received:', room123Subscriber.receivedMessages.length);
  console.log('📊 Room 456 subscriber received:', room456Subscriber.receivedMessages.length);
  
  // Test assertions - each room should only receive its own events
  if (room123Subscriber.receivedMessages.length !== 1) {
    throw new Error(`Room 123 should receive 1 event, got ${room123Subscriber.receivedMessages.length}`);
  }
  
  if (room456Subscriber.receivedMessages.length !== 1) {
    throw new Error(`Room 456 should receive 1 event, got ${room456Subscriber.receivedMessages.length}`);
  }
  
  // Verify event content isolation
  const room123Message = room123Subscriber.receivedMessages[0];
  const room456Message = room456Subscriber.receivedMessages[0];
  
  if ((room123Message.payload as any).content !== 'Hello room 123') {
    throw new Error('Room 123 received wrong message content');
  }
  
  if ((room456Message.payload as any).content !== 'Hello room 456') {
    throw new Error('Room 456 received wrong message content');
  }
  
  console.log('✅ UNIT TEST PASSED: Room-scoped event routing working via path subscription');
  return true;
}

// Run test
testRoomScopedEventRouting().catch(error => {
  console.error('❌ UNIT TEST FAILED:', error);
  process.exit(1);
});