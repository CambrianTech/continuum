/**
 * PersonaInbox Unit Tests
 *
 * Tests the priority-based message queue for autonomous personas
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { PersonaInbox, calculateMessagePriority, type InboxMessage } from '../../system/user/server/modules/PersonaInbox';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

describe('PersonaInbox', () => {
  let inbox: PersonaInbox;
  const personaId: UUID = 'test-persona-id' as UUID;
  const personaName = 'TestPersona';

  beforeEach(() => {
    inbox = new PersonaInbox(personaId, personaName, {
      maxSize: 10,
      enableLogging: false
    });
  });

  describe('Configuration', () => {
    it('should initialize with default config', () => {
      const defaultInbox = new PersonaInbox(personaId, personaName);
      expect(defaultInbox.getSize()).toBe(0);
    });

    it('should initialize with custom config', () => {
      const customInbox = new PersonaInbox(personaId, personaName, {
        maxSize: 5,
        enableLogging: false
      });
      expect(customInbox.getSize()).toBe(0);
    });
  });

  describe('Basic Operations', () => {
    it('should enqueue messages', async () => {
      const message: InboxMessage = {
        messageId: 'msg-1',
        roomId: 'room-1' as UUID,
        content: 'Test message',
        senderId: 'user-1' as UUID,
        senderName: 'User',
        timestamp: Date.now(),
        priority: 0.5
      };

      const result = await inbox.enqueue(message);
      expect(result).toBe(true);
      expect(inbox.getSize()).toBe(1);
    });

    it('should peek without removing', async () => {
      const message: InboxMessage = {
        messageId: 'msg-1',
        roomId: 'room-1' as UUID,
        content: 'Test message',
        senderId: 'user-1' as UUID,
        senderName: 'User',
        timestamp: Date.now(),
        priority: 0.5
      };

      await inbox.enqueue(message);
      const peeked = await inbox.peek(1);

      expect(peeked).toHaveLength(1);
      expect(peeked[0].messageId).toBe('msg-1');
      expect(inbox.getSize()).toBe(1); // Not removed
    });

    it('should pop messages', async () => {
      const message: InboxMessage = {
        messageId: 'msg-1',
        roomId: 'room-1' as UUID,
        content: 'Test message',
        senderId: 'user-1' as UUID,
        senderName: 'User',
        timestamp: Date.now(),
        priority: 0.5
      };

      await inbox.enqueue(message);
      const popped = await inbox.pop(100);

      expect(popped?.messageId).toBe('msg-1');
      expect(inbox.getSize()).toBe(0); // Removed
    });

    it('should clear all messages', async () => {
      const messages: InboxMessage[] = [
        {
          messageId: 'msg-1',
          roomId: 'room-1' as UUID,
          content: 'Test 1',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.5
        },
        {
          messageId: 'msg-2',
          roomId: 'room-1' as UUID,
          content: 'Test 2',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.3
        }
      ];

      for (const msg of messages) {
        await inbox.enqueue(msg);
      }

      expect(inbox.getSize()).toBe(2);
      inbox.clear();
      expect(inbox.getSize()).toBe(0);
    });
  });

  describe('Priority Ordering', () => {
    it('should sort by priority (highest first)', async () => {
      const messages: InboxMessage[] = [
        {
          messageId: 'msg-low',
          roomId: 'room-1' as UUID,
          content: 'Low priority',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.2
        },
        {
          messageId: 'msg-high',
          roomId: 'room-1' as UUID,
          content: 'High priority',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.9
        },
        {
          messageId: 'msg-medium',
          roomId: 'room-1' as UUID,
          content: 'Medium priority',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.5
        }
      ];

      for (const msg of messages) {
        await inbox.enqueue(msg);
      }

      const peeked = await inbox.peek(3);
      expect(peeked[0].messageId).toBe('msg-high');    // 0.9
      expect(peeked[1].messageId).toBe('msg-medium');  // 0.5
      expect(peeked[2].messageId).toBe('msg-low');     // 0.2
    });
  });

  describe('Traffic Management', () => {
    it('should drop lowest priority when full', async () => {
      const smallInbox = new PersonaInbox(personaId, personaName, {
        maxSize: 3,
        enableLogging: false
      });

      // Fill inbox
      await smallInbox.enqueue({
        messageId: 'msg-1',
        roomId: 'room-1' as UUID,
        content: 'Priority 0.2',
        senderId: 'user-1' as UUID,
        senderName: 'User',
        timestamp: Date.now(),
        priority: 0.2
      });
      await smallInbox.enqueue({
        messageId: 'msg-2',
        roomId: 'room-1' as UUID,
        content: 'Priority 0.5',
        senderId: 'user-1' as UUID,
        senderName: 'User',
        timestamp: Date.now(),
        priority: 0.5
      });
      await smallInbox.enqueue({
        messageId: 'msg-3',
        roomId: 'room-1' as UUID,
        content: 'Priority 0.8',
        senderId: 'user-1' as UUID,
        senderName: 'User',
        timestamp: Date.now(),
        priority: 0.8
      });

      expect(smallInbox.getSize()).toBe(3);

      // Add high priority message (should drop lowest)
      await smallInbox.enqueue({
        messageId: 'msg-4',
        roomId: 'room-1' as UUID,
        content: 'Priority 0.9',
        senderId: 'user-1' as UUID,
        senderName: 'User',
        timestamp: Date.now(),
        priority: 0.9
      });

      expect(smallInbox.getSize()).toBe(3); // Still 3
      const peeked = await smallInbox.peek(3);
      expect(peeked.some(m => m.messageId === 'msg-1')).toBe(false); // Lowest dropped
      expect(peeked.some(m => m.messageId === 'msg-4')).toBe(true);  // High priority kept
    });
  });

  describe('Load Awareness', () => {
    it('should calculate load percentage', async () => {
      const smallInbox = new PersonaInbox(personaId, personaName, {
        maxSize: 10,
        enableLogging: false
      });

      expect(smallInbox.getLoad()).toBe(0.0);

      // Add 5 messages (50% full)
      for (let i = 0; i < 5; i++) {
        await smallInbox.enqueue({
          messageId: `msg-${i}`,
          roomId: 'room-1' as UUID,
          content: `Message ${i}`,
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.5
        });
      }

      expect(smallInbox.getLoad()).toBe(0.5);
    });

    it('should detect overload (>75%)', async () => {
      const smallInbox = new PersonaInbox(personaId, personaName, {
        maxSize: 10,
        enableLogging: false
      });

      expect(smallInbox.isOverloaded()).toBe(false);

      // Add 8 messages (80% full)
      for (let i = 0; i < 8; i++) {
        await smallInbox.enqueue({
          messageId: `msg-${i}`,
          roomId: 'room-1' as UUID,
          content: `Message ${i}`,
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.5
        });
      }

      expect(smallInbox.isOverloaded()).toBe(true);
    });
  });

  describe('Stats', () => {
    it('should provide inbox stats', async () => {
      const messages: InboxMessage[] = [
        {
          messageId: 'msg-1',
          roomId: 'room-1' as UUID,
          content: 'Low',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.2
        },
        {
          messageId: 'msg-2',
          roomId: 'room-1' as UUID,
          content: 'High',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.9
        }
      ];

      for (const msg of messages) {
        await inbox.enqueue(msg);
      }

      const stats = inbox.getStats();
      expect(stats.size).toBe(2);
      expect(stats.load).toBe(0.2); // 2/10
      expect(stats.overloaded).toBe(false);
      expect(stats.highestPriority).toBe(0.9);
      expect(stats.lowestPriority).toBe(0.2);
    });
  });

  describe('Timeout Behavior', () => {
    it('should return null on timeout', async () => {
      const result = await inbox.pop(50); // 50ms timeout
      expect(result).toBeNull();
    });

    it('should wait for message within timeout', async () => {
      // Enqueue after 25ms
      setTimeout(async () => {
        await inbox.enqueue({
          messageId: 'msg-1',
          roomId: 'room-1' as UUID,
          content: 'Delayed',
          senderId: 'user-1' as UUID,
          senderName: 'User',
          timestamp: Date.now(),
          priority: 0.5
        });
      }, 25);

      const result = await inbox.pop(100); // 100ms timeout
      expect(result).not.toBeNull();
      expect(result?.messageId).toBe('msg-1');
    });
  });
});

describe('calculateMessagePriority', () => {
  const persona = {
    displayName: 'TestBot',
    id: 'test-persona-id' as UUID,
    recentRooms: ['room-1' as UUID],
    expertise: ['typescript', 'testing']
  };

  describe('Base Priority', () => {
    it('should have baseline priority of 0.2', () => {
      const message = {
        content: 'Regular message',
        timestamp: Date.now(),
        roomId: 'room-1' as UUID
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeGreaterThanOrEqual(0.2);
    });
  });

  describe('Mention Priority', () => {
    it('should add 0.4 for @mention', () => {
      const message = {
        content: '@TestBot can you help?',
        timestamp: Date.now(),
        roomId: 'room-1' as UUID
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeGreaterThanOrEqual(0.6); // 0.2 base + 0.4 mention
    });

    it('should be case-insensitive', () => {
      const message = {
        content: '@testbot lowercase mention',
        timestamp: Date.now(),
        roomId: 'room-1' as UUID
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeGreaterThanOrEqual(0.6);
    });
  });

  describe('Recency Priority', () => {
    it('should add 0.2 for messages < 1 minute old', () => {
      const message = {
        content: 'Recent message',
        timestamp: Date.now() - 30000, // 30 seconds ago
        roomId: 'room-2' as UUID
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeGreaterThanOrEqual(0.4); // 0.2 base + 0.2 recent
    });

    it('should add 0.1 for messages < 5 minutes old', () => {
      const message = {
        content: 'Somewhat recent',
        timestamp: Date.now() - 180000, // 3 minutes ago
        roomId: 'room-2' as UUID
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeGreaterThanOrEqual(0.3); // 0.2 base + 0.1 recent
    });

    it('should not add recency bonus for old messages', () => {
      const message = {
        content: 'Old message',
        timestamp: Date.now() - 600000, // 10 minutes ago
        roomId: 'room-2' as UUID
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBe(0.2); // Just base
    });
  });

  describe('Active Conversation Priority', () => {
    it('should add 0.1 for messages in recent rooms', () => {
      const message = {
        content: 'Message in active room',
        timestamp: Date.now() - 600000, // Old message
        roomId: 'room-1' as UUID // In recentRooms
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeGreaterThanOrEqual(0.3); // 0.2 base + 0.1 active
    });
  });

  describe('Expertise Priority', () => {
    it('should add 0.1 for relevant expertise keywords', () => {
      const message = {
        content: 'Need help with typescript testing',
        timestamp: Date.now() - 600000, // Old
        roomId: 'room-2' as UUID // Not in recentRooms
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeGreaterThanOrEqual(0.3); // 0.2 base + 0.1 expertise
    });
  });

  describe('Combined Priority', () => {
    it('should combine all factors', () => {
      const message = {
        content: '@TestBot help with typescript',
        timestamp: Date.now() - 30000, // Recent
        roomId: 'room-1' as UUID // Active room
      };

      const priority = calculateMessagePriority(message, persona);
      // 0.2 base + 0.4 mention + 0.2 recent + 0.1 active + 0.1 expertise = 1.0
      expect(priority).toBe(1.0);
    });

    it('should cap at 1.0', () => {
      const message = {
        content: '@TestBot @TestBot double mention typescript',
        timestamp: Date.now(),
        roomId: 'room-1' as UUID
      };

      const priority = calculateMessagePriority(message, persona);
      expect(priority).toBeLessThanOrEqual(1.0);
    });
  });
});
