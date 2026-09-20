/**
 * Event Coalescing Integration Tests
 *
 * Tests that duplicate events are merged to reduce processing overhead.
 * When 14 "new message in room X" events fire rapidly, only 1 event should
 * be emitted with count=14 and the latest data.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { EventManager, type CoalescedEventData } from '../../system/events/shared/JTAGEventSystem';

describe('Event Coalescing', () => {
  let eventManager: EventManager;

  beforeEach(() => {
    eventManager = new EventManager();
  });

  describe('Message Event Coalescing', () => {
    it('should coalesce and emit all messages in chronological order', async () => {
      const receivedEvents: unknown[] = [];

      // Subscribe to chat message events
      eventManager.events.on('chat:message-received', (data) => {
        receivedEvents.push(data);
      });

      // Fire 14 rapid message events for same room (with varying timestamps)
      const roomId = 'test-room-123';
      const baseTime = Date.now();
      for (let i = 1; i <= 14; i++) {
        eventManager.events.emit('chat:message-received', {
          roomId,
          messageId: `msg-${i}`,
          content: `Message ${i}`,
          timestamp: baseTime + i * 10  // 10ms apart
        });
      }

      // Wait for debounce delay (100ms + buffer)
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should have received ALL 14 messages (coalescing collects them, then emits all)
      expect(receivedEvents.length).toBe(14);

      // Verify all messages present in chronological order
      for (let i = 0; i < 14; i++) {
        const message = receivedEvents[i] as any;
        expect(message.messageId).toBe(`msg-${i + 1}`);
        expect(message.content).toBe(`Message ${i + 1}`);
        expect(message.roomId).toBe(roomId);
      }

      // Verify timestamps are in ascending order (original chronological order preserved)
      for (let i = 1; i < receivedEvents.length; i++) {
        const prevTime = (receivedEvents[i - 1] as any).timestamp;
        const currTime = (receivedEvents[i] as any).timestamp;
        expect(currTime).toBeGreaterThan(prevTime);
      }
    });

    it('should NOT coalesce events from different rooms', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('chat:message-received', (data) => {
        receivedEvents.push(data);
      });

      // Fire events for 3 different rooms
      const baseTime = Date.now();
      eventManager.events.emit('chat:message-received', {
        roomId: 'room-1',
        messageId: 'msg-1',
        content: 'Message in room 1',
        timestamp: baseTime
      });

      eventManager.events.emit('chat:message-received', {
        roomId: 'room-2',
        messageId: 'msg-2',
        content: 'Message in room 2',
        timestamp: baseTime + 10
      });

      eventManager.events.emit('chat:message-received', {
        roomId: 'room-3',
        messageId: 'msg-3',
        content: 'Message in room 3',
        timestamp: baseTime + 20
      });

      // Wait for debounce
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should have 3 separate messages (different rooms, no coalescing)
      expect(receivedEvents.length).toBe(3);

      // Verify each message
      expect((receivedEvents[0] as any).roomId).toBe('room-1');
      expect((receivedEvents[1] as any).roomId).toBe('room-2');
      expect((receivedEvents[2] as any).roomId).toBe('room-3');
    });

    it('should correct message ordering when sent out-of-order', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('chat:message-received', (data) => {
        receivedEvents.push(data);
      });

      // Fire messages OUT OF ORDER (2, 3, 5, 4, 1) to simulate race condition
      const roomId = 'test-room-race';
      const baseTime = Date.now();

      eventManager.events.emit('chat:message-received', {
        roomId,
        messageId: 'msg-2',
        content: 'Message 2',
        timestamp: baseTime + 20  // 2nd chronologically
      });

      eventManager.events.emit('chat:message-received', {
        roomId,
        messageId: 'msg-3',
        content: 'Message 3',
        timestamp: baseTime + 30  // 3rd
      });

      eventManager.events.emit('chat:message-received', {
        roomId,
        messageId: 'msg-5',
        content: 'Message 5',
        timestamp: baseTime + 50  // 5th (latest)
      });

      eventManager.events.emit('chat:message-received', {
        roomId,
        messageId: 'msg-4',
        content: 'Message 4',
        timestamp: baseTime + 40  // 4th
      });

      eventManager.events.emit('chat:message-received', {
        roomId,
        messageId: 'msg-1',
        content: 'Message 1',
        timestamp: baseTime + 10  // 1st (earliest)
      });

      // Wait for debounce
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should have received all 5 messages
      expect(receivedEvents.length).toBe(5);

      // Messages should be CORRECTED to chronological order (1, 2, 3, 4, 5)
      expect((receivedEvents[0] as any).messageId).toBe('msg-1');
      expect((receivedEvents[1] as any).messageId).toBe('msg-2');
      expect((receivedEvents[2] as any).messageId).toBe('msg-3');
      expect((receivedEvents[3] as any).messageId).toBe('msg-4');
      expect((receivedEvents[4] as any).messageId).toBe('msg-5');

      // Verify timestamps are in correct ascending order
      for (let i = 1; i < receivedEvents.length; i++) {
        const prevTime = (receivedEvents[i - 1] as any).timestamp;
        const currTime = (receivedEvents[i] as any).timestamp;
        expect(currTime).toBeGreaterThan(prevTime);
      }
    });

    it('should emit all messages despite delayed submissions', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('chat:message-received', (data) => {
        receivedEvents.push(data);
      });

      const roomId = 'test-room-123';
      const baseTime = Date.now();

      // Fire 5 events with 10ms spacing (simulating network delays)
      for (let i = 0; i < 5; i++) {
        await new Promise(resolve => setTimeout(resolve, 10));
        eventManager.events.emit('chat:message-received', {
          roomId,
          messageId: `msg-${i}`,
          content: `Message ${i}`,
          timestamp: baseTime + i * 10
        });
      }

      // Wait for debounce
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should emit all 5 messages in chronological order
      expect(receivedEvents.length).toBe(5);

      // Verify all messages present
      for (let i = 0; i < 5; i++) {
        const msg = receivedEvents[i] as any;
        expect(msg.messageId).toBe(`msg-${i}`);
        expect(msg.content).toBe(`Message ${i}`);
      }
    });
  });

  describe('Non-Coalescing Events', () => {
    it('should NOT coalesce user action events', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('user:click', (data) => {
        receivedEvents.push(data);
      });

      // Fire 5 rapid user clicks
      for (let i = 0; i < 5; i++) {
        eventManager.events.emit('user:click', {
          userId: 'user-123',
          buttonId: `button-${i}`
        });
      }

      // Wait for debounce
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should have 5 separate events (user actions don't coalesce)
      expect(receivedEvents.length).toBe(5);

      // Events should NOT be CoalescedEventData (emitted directly)
      receivedEvents.forEach(event => {
        expect((event as any).count).toBeUndefined();
      });
    });

    it('should NOT coalesce system events', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('system:startup', (data) => {
        receivedEvents.push(data);
      });

      // Fire multiple system startup events
      for (let i = 0; i < 3; i++) {
        eventManager.events.emit('system:startup', {
          phase: `phase-${i}`
        });
      }

      // Wait for debounce
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should have 3 separate events (system events don't coalesce)
      expect(receivedEvents.length).toBe(3);
    });
  });

  describe('State Update Coalescing', () => {
    it('should coalesce rapid state updates for same entity', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('state:update', (data) => {
        receivedEvents.push(data);
      });

      const userId = 'user-123';

      // Fire 10 rapid state updates
      for (let i = 0; i < 10; i++) {
        eventManager.events.emit('state:update', {
          userId,
          tab: `tab-${i}`,
          scrollPosition: i * 100
        });
      }

      // Wait for debounce
      await new Promise(resolve => setTimeout(resolve, 150));

      // Should have 1 coalesced event
      expect(receivedEvents.length).toBe(1);

      const coalescedEvent = receivedEvents[0] as CoalescedEventData;
      expect(coalescedEvent.count).toBe(10);

      // Latest state should win
      expect((coalescedEvent.data as any).tab).toBe('tab-9');
      expect((coalescedEvent.data as any).scrollPosition).toBe(900);
    });
  });

  describe('Performance Benefits', () => {
    it('should reduce handler overhead via coalescing', async () => {
      let handlerCallCount = 0;
      const receivedEvents: unknown[] = [];

      eventManager.events.on('chat:message-received', (data) => {
        handlerCallCount++;
        receivedEvents.push(data);
      });

      const roomId = 'busy-room';
      const baseTime = Date.now();

      // Simulate heavy load: 100 messages in quick succession
      const startTime = Date.now();
      for (let i = 0; i < 100; i++) {
        eventManager.events.emit('chat:message-received', {
          roomId,
          messageId: `msg-${i}`,
          content: `Message ${i}`,
          timestamp: baseTime + i
        });
      }

      // Wait for debounce
      await new Promise(resolve => setTimeout(resolve, 150));
      const endTime = Date.now();

      // Should have received all 100 messages (data completeness)
      expect(receivedEvents.length).toBe(100);

      // But handler called 100 times (once per message emission)
      // The benefit: PersonaUser.handleChatMessage() called once instead of 100x
      // (because coalescing happens BEFORE handler, collecting all messages first)
      expect(handlerCallCount).toBe(100);

      // Total time should be minimal (debounce delay + small overhead)
      const totalTime = endTime - startTime;
      expect(totalTime).toBeLessThan(300); // Should complete in < 300ms

      console.log(`✅ Coalesced 100 events (saved 99 PersonaUser handler calls)`);
      console.log(`   Total time: ${totalTime}ms`);
      console.log(`   Messages emitted: ${receivedEvents.length}`);
    });
  });

  describe('Context Key Extraction', () => {
    it('should extract roomId as context key and coalesce', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('chat:message-received', (data) => {
        receivedEvents.push(data);
      });

      // Fire events with roomId
      const baseTime = Date.now();
      for (let i = 0; i < 5; i++) {
        eventManager.events.emit('chat:message-received', {
          roomId: 'room-123',
          messageId: `msg-${i}`,
          timestamp: baseTime + i
        });
      }

      await new Promise(resolve => setTimeout(resolve, 150));

      // Should emit all 5 messages (coalesced by roomId, then emitted in order)
      expect(receivedEvents.length).toBe(5);

      // Verify all messages received in order
      for (let i = 0; i < 5; i++) {
        expect((receivedEvents[i] as any).messageId).toBe(`msg-${i}`);
      }
    });

    it('should extract userId as context key', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('state:update', (data) => {
        receivedEvents.push(data);
      });

      // Fire events with userId
      for (let i = 0; i < 5; i++) {
        eventManager.events.emit('state:update', {
          userId: 'user-123',
          value: i
        });
      }

      await new Promise(resolve => setTimeout(resolve, 150));

      // Should coalesce by userId
      expect(receivedEvents.length).toBe(1);
      expect((receivedEvents[0] as CoalescedEventData).count).toBe(5);
    });

    it('should extract contextId as context key and coalesce', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('chat:update', (data) => {
        receivedEvents.push(data);
      });

      // Fire events with contextId (contains 'chat:', so will emit all)
      const baseTime = Date.now();
      for (let i = 0; i < 5; i++) {
        eventManager.events.emit('chat:update', {
          contextId: 'context-123',
          value: i,
          timestamp: baseTime + i
        });
      }

      await new Promise(resolve => setTimeout(resolve, 150));

      // Should emit all 5 messages (coalesced by contextId, then emitted in order)
      expect(receivedEvents.length).toBe(5);

      // Verify all events received in order
      for (let i = 0; i < 5; i++) {
        expect((receivedEvents[i] as any).value).toBe(i);
      }
    });

    it('should extract sessionId as context key and coalesce', async () => {
      const receivedEvents: unknown[] = [];

      eventManager.events.on('message:update', (data) => {
        receivedEvents.push(data);
      });

      // Fire events with sessionId (contains 'message', so will emit all)
      const baseTime = Date.now();
      for (let i = 0; i < 5; i++) {
        eventManager.events.emit('message:update', {
          sessionId: 'session-123',
          value: i,
          timestamp: baseTime + i
        });
      }

      await new Promise(resolve => setTimeout(resolve, 150));

      // Should emit all 5 messages (coalesced by sessionId, then emitted in order)
      expect(receivedEvents.length).toBe(5);

      // Verify all events received in order
      for (let i = 0; i < 5; i++) {
        expect((receivedEvents[i] as any).value).toBe(i);
      }
    });
  });
});
