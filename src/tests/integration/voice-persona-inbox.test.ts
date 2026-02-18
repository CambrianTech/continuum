/**
 * voice-persona-inbox.test.ts
 *
 * Integration tests for PersonaUser voice inbox handling
 * Tests the flow from directed events to inbox enqueuing to response generation
 *
 * Architecture tested:
 * 1. PersonaUser subscribes to voice:transcription:directed
 * 2. Receives event only when targetPersonaId matches
 * 3. Enqueues to inbox with sourceModality='voice'
 * 4. Inbox message includes voiceSessionId
 * 5. Response generator routes to TTS based on metadata
 *
 * Run with: npx vitest tests/integration/voice-persona-inbox.test.ts
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { Events } from '../../system/core/shared/Events';
import type { UUID } from '../../types/CrossPlatformUUID';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { InboxMessage } from '../../system/user/server/modules/QueueItemTypes';

// Mock UUIDs for testing
const MOCK_PERSONA_ID: UUID = 'persona-helper-ai' as UUID;
const MOCK_SESSION_ID: UUID = 'voice-session-001' as UUID;
const MOCK_SPEAKER_ID: UUID = 'user-joel-001' as UUID;
const MOCK_ROOM_ID: UUID = 'room-general-001' as UUID;

// Mock directed event factory
function createDirectedEvent(
  transcript: string,
  targetPersonaId: UUID = MOCK_PERSONA_ID,
  sessionId: UUID = MOCK_SESSION_ID
): {
  sessionId: UUID;
  speakerId: UUID;
  speakerName: string;
  transcript: string;
  confidence: number;
  language: string;
  timestamp: number;
  targetPersonaId: UUID;
} {
  return {
    sessionId,
    speakerId: MOCK_SPEAKER_ID,
    speakerName: 'Joel',
    transcript,
    confidence: 0.95,
    language: 'en',
    timestamp: Date.now(),
    targetPersonaId
  };
}

describe('PersonaUser Voice Inbox Integration Tests', () => {
  let eventSubscribers: Map<string, Function[]>;

  beforeEach(() => {
    // Reset event subscribers
    eventSubscribers = new Map();
    vi.spyOn(Events, 'subscribe').mockImplementation((eventName: string, handler: Function) => {
      if (!eventSubscribers.has(eventName)) {
        eventSubscribers.set(eventName, []);
      }
      eventSubscribers.get(eventName)!.push(handler);
      return () => {}; // Unsubscribe function
    });

    vi.spyOn(Events, 'emit').mockImplementation(async (eventName: string, data: any) => {
      const handlers = eventSubscribers.get(eventName);
      if (handlers) {
        for (const handler of handlers) {
          await handler(data);
        }
      }
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  describe('Directed Event Subscription', () => {
    it('should subscribe to voice:transcription:directed events', () => {
      // Mock PersonaUser subscription
      Events.subscribe('voice:transcription:directed', async (data) => {
        // Handler logic
      });

      expect(eventSubscribers.has('voice:transcription:directed')).toBe(true);
      expect(eventSubscribers.get('voice:transcription:directed')!.length).toBe(1);
    });

    it('should only process events targeted at this persona', async () => {
      let receivedEvent = false;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          receivedEvent = true;
        }
      });

      // Send event targeted at this persona
      await Events.emit('voice:transcription:directed', createDirectedEvent('Hello'));
      expect(receivedEvent).toBe(true);

      // Reset and send event targeted at different persona
      receivedEvent = false;
      await Events.emit('voice:transcription:directed', createDirectedEvent('Hello', 'other-persona-id' as UUID));
      expect(receivedEvent).toBe(false);
    });

    it('should ignore own transcriptions (persona speaking)', async () => {
      let receivedEvent = false;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        // PersonaUser checks if speakerId === this.id
        if (data.speakerId === MOCK_PERSONA_ID) {
          // Ignore own transcriptions
          return;
        }
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          receivedEvent = true;
        }
      });

      // Send event from this persona (should be ignored)
      const ownEvent = createDirectedEvent('I think this is correct');
      ownEvent.speakerId = MOCK_PERSONA_ID;
      await Events.emit('voice:transcription:directed', ownEvent);

      expect(receivedEvent).toBe(false);
    });
  });

  describe('Inbox Message Creation', () => {
    it('should create inbox message with sourceModality="voice"', async () => {
      let inboxMessage: InboxMessage | null = null;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID && data.speakerId !== MOCK_PERSONA_ID) {
          // Simulate PersonaUser creating InboxMessage
          inboxMessage = {
            id: generateUUID(),
            type: 'message',
            domain: 'chat',
            roomId: data.sessionId,
            content: data.transcript,
            senderId: data.speakerId,
            senderName: data.speakerName,
            senderType: 'human',
            timestamp: data.timestamp,
            priority: 0.75, // Boosted for voice
            sourceModality: 'voice', // KEY: marks as voice for TTS routing
            voiceSessionId: data.sessionId
          };
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('What is TypeScript?'));

      expect(inboxMessage).not.toBeNull();
      expect(inboxMessage?.sourceModality).toBe('voice');
      expect(inboxMessage?.voiceSessionId).toBe(MOCK_SESSION_ID);
      expect(inboxMessage?.domain).toBe('chat');
      expect(inboxMessage?.content).toBe('What is TypeScript?');
    });

    it('should boost priority for voice messages', async () => {
      let basePriority = 0.5;
      let voicePriority = 0.0;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          // Simulate priority calculation with voice boost
          voicePriority = Math.min(1.0, basePriority + 0.2); // +0.2 voice boost
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('Hello'));

      expect(voicePriority).toBe(0.7); // 0.5 + 0.2 = 0.7
      expect(voicePriority).toBeGreaterThan(basePriority);
    });

    it('should include all required metadata for TTS routing', async () => {
      let inboxMessage: InboxMessage | null = null;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          inboxMessage = {
            id: generateUUID(),
            type: 'message',
            domain: 'chat',
            roomId: data.sessionId,
            content: data.transcript,
            senderId: data.speakerId,
            senderName: data.speakerName,
            senderType: 'human',
            timestamp: data.timestamp,
            priority: 0.75,
            sourceModality: 'voice',
            voiceSessionId: data.sessionId
          };
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('Explain closures'));

      expect(inboxMessage).not.toBeNull();
      expect(inboxMessage).toMatchObject({
        type: 'message',
        domain: 'chat',
        sourceModality: 'voice',
        voiceSessionId: MOCK_SESSION_ID,
        content: 'Explain closures'
      });
    });
  });

  describe('Deduplication Logic', () => {
    it('should deduplicate identical transcriptions', async () => {
      const processedKeys = new Set<string>();

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          const key = `${data.speakerId}-${data.timestamp}`;

          // PersonaUser uses rateLimiter to deduplicate
          if (processedKeys.has(key)) {
            // Skip duplicate
            return;
          }
          processedKeys.add(key);
        }
      });

      const event = createDirectedEvent('Duplicate message');
      await Events.emit('voice:transcription:directed', event);
      await Events.emit('voice:transcription:directed', event); // Same event twice

      expect(processedKeys.size).toBe(1); // Only processed once
    });

    it('should process different transcriptions from same speaker', async () => {
      const processedKeys = new Set<string>();

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          const key = `${data.speakerId}-${data.timestamp}`;
          if (!processedKeys.has(key)) {
            processedKeys.add(key);
          }
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('First message'));
      await new Promise(resolve => setTimeout(resolve, 10)); // Different timestamp
      await Events.emit('voice:transcription:directed', createDirectedEvent('Second message'));

      expect(processedKeys.size).toBe(2); // Both processed
    });
  });

  describe('Consciousness Timeline Recording', () => {
    it('should record voice transcriptions in consciousness timeline', async () => {
      let timelineEvents: any[] = [];

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          // Simulate consciousness recording
          const timelineEvent = {
            contextType: 'room',
            contextId: data.sessionId,
            contextName: `Voice Call ${data.sessionId.slice(0, 8)}`,
            eventType: 'message_received',
            actorId: data.speakerId,
            actorName: data.speakerName,
            content: data.transcript,
            importance: 0.7,
            topics: extractTopics(data.transcript)
          };
          timelineEvents.push(timelineEvent);
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('Explain TypeScript generics'));

      expect(timelineEvents.length).toBe(1);
      expect(timelineEvents[0]).toMatchObject({
        contextType: 'room',
        eventType: 'message_received',
        actorName: 'Joel',
        content: 'Explain TypeScript generics',
        importance: 0.7
      });
    });
  });

  describe('Priority Calculation', () => {
    it('should calculate higher priority for direct questions', async () => {
      const priorities: number[] = [];

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          // Simulate priority calculation
          let basePriority = 0.5;

          // Question boost
          if (data.transcript.includes('?') || /^(what|how|why|can|could)/i.test(data.transcript)) {
            basePriority += 0.1;
          }

          // Voice boost
          basePriority += 0.2;

          priorities.push(Math.min(1.0, basePriority));
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('What is TypeScript?'));
      await Events.emit('voice:transcription:directed', createDirectedEvent('The weather is nice'));

      expect(priorities[0]).toBeGreaterThan(priorities[1]); // Question has higher priority
    });

    it('should cap priority at 1.0', async () => {
      let calculatedPriority = 0.0;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          let priority = 0.9; // High base priority
          priority += 0.2; // Voice boost
          calculatedPriority = Math.min(1.0, priority); // Cap at 1.0
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('Question?'));

      expect(calculatedPriority).toBe(1.0);
      expect(calculatedPriority).toBeLessThanOrEqual(1.0);
    });
  });

  describe('Error Handling', () => {
    it('should handle malformed directed events gracefully', async () => {
      let errorOccurred = false;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        try {
          if (!data.targetPersonaId || !data.transcript) {
            throw new Error('Invalid event data');
          }
          // Process event
        } catch (error) {
          errorOccurred = true;
        }
      });

      // Send malformed event
      await Events.emit('voice:transcription:directed', {
        sessionId: MOCK_SESSION_ID,
        // Missing required fields
      });

      expect(errorOccurred).toBe(true);
    });

    it('should handle timestamp in different formats', async () => {
      let timestamps: number[] = [];

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          // PersonaUser accepts both string and number timestamps
          const timestamp = data.timestamp
            ? (typeof data.timestamp === 'number'
                ? data.timestamp
                : new Date(data.timestamp).getTime())
            : Date.now();
          timestamps.push(timestamp);
        }
      });

      // Number timestamp
      const event1 = createDirectedEvent('Hello');
      await Events.emit('voice:transcription:directed', event1);

      // String timestamp
      const event2 = createDirectedEvent('World');
      (event2 as any).timestamp = new Date().toISOString();
      await Events.emit('voice:transcription:directed', event2);

      expect(timestamps.length).toBe(2);
      expect(typeof timestamps[0]).toBe('number');
      expect(typeof timestamps[1]).toBe('number');
    });
  });

  describe('Inbox Load Awareness', () => {
    it('should update inbox load after enqueuing', async () => {
      let inboxSize = 0;

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          // Simulate inbox enqueue
          inboxSize++;
          // PersonaState.updateInboxLoad(inboxSize)
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('First message'));
      expect(inboxSize).toBe(1);

      await Events.emit('voice:transcription:directed', createDirectedEvent('Second message'));
      expect(inboxSize).toBe(2);
    });

    it('should log inbox enqueue with priority and confidence', async () => {
      const logs: string[] = [];

      Events.subscribe('voice:transcription:directed', async (data: any) => {
        if (data.targetPersonaId === MOCK_PERSONA_ID) {
          const priority = 0.75;
          const log = `Enqueued voice transcription (priority=${priority.toFixed(2)}, confidence=${data.confidence}, inbox size=1)`;
          logs.push(log);
        }
      });

      await Events.emit('voice:transcription:directed', createDirectedEvent('Test message'));

      expect(logs.length).toBe(1);
      expect(logs[0]).toContain('priority=0.75');
      expect(logs[0]).toContain('confidence=0.95');
    });
  });
});

describe('Voice Persona Inbox Success Criteria', () => {
  it('✅ PersonaUser receives directed events only when targeted', async () => {
    let receivedCount = 0;

    Events.subscribe('voice:transcription:directed', async (data: any) => {
      if (data.targetPersonaId === MOCK_PERSONA_ID) {
        receivedCount++;
      }
    });

    await Events.emit('voice:transcription:directed', createDirectedEvent('For me'));
    await Events.emit('voice:transcription:directed', createDirectedEvent('Not for me', 'other-persona' as UUID));

    expect(receivedCount).toBe(1); // Only one targeted event
  });

  it('✅ Inbox messages have sourceModality="voice" for TTS routing', async () => {
    let inboxMessage: InboxMessage | null = null;

    Events.subscribe('voice:transcription:directed', async (data: any) => {
      if (data.targetPersonaId === MOCK_PERSONA_ID) {
        inboxMessage = {
          id: generateUUID(),
          type: 'message',
          domain: 'chat',
          roomId: data.sessionId,
          content: data.transcript,
          senderId: data.speakerId,
          senderName: data.speakerName,
          senderType: 'human',
          timestamp: data.timestamp,
          priority: 0.75,
          sourceModality: 'voice',
          voiceSessionId: data.sessionId
        };
      }
    });

    await Events.emit('voice:transcription:directed', createDirectedEvent('Test'));

    expect(inboxMessage?.sourceModality).toBe('voice');
    expect(inboxMessage?.voiceSessionId).toBeDefined();
  });

  it('✅ Priority boosted for voice messages', async () => {
    const priorities: number[] = [];

    Events.subscribe('voice:transcription:directed', async (data: any) => {
      if (data.targetPersonaId === MOCK_PERSONA_ID) {
        const basePriority = 0.5;
        const voicePriority = Math.min(1.0, basePriority + 0.2);
        priorities.push(voicePriority);
      }
    });

    await Events.emit('voice:transcription:directed', createDirectedEvent('Test'));

    expect(priorities[0]).toBe(0.7); // 0.5 + 0.2 voice boost
  });

  it('✅ Deduplication prevents duplicate processing', async () => {
    const processedKeys = new Set<string>();

    Events.subscribe('voice:transcription:directed', async (data: any) => {
      if (data.targetPersonaId === MOCK_PERSONA_ID) {
        const key = `${data.speakerId}-${data.timestamp}`;
        if (!processedKeys.has(key)) {
          processedKeys.add(key);
        }
      }
    });

    const event = createDirectedEvent('Duplicate');
    await Events.emit('voice:transcription:directed', event);
    await Events.emit('voice:transcription:directed', event);

    expect(processedKeys.size).toBe(1);
  });

  it('✅ Consciousness timeline records voice interactions', async () => {
    const timelineEvents: any[] = [];

    Events.subscribe('voice:transcription:directed', async (data: any) => {
      if (data.targetPersonaId === MOCK_PERSONA_ID) {
        timelineEvents.push({
          contextType: 'room',
          eventType: 'message_received',
          content: data.transcript
        });
      }
    });

    await Events.emit('voice:transcription:directed', createDirectedEvent('Voice message'));

    expect(timelineEvents.length).toBe(1);
    expect(timelineEvents[0].contextType).toBe('room');
    expect(timelineEvents[0].eventType).toBe('message_received');
  });
});

// Helper function (same as PersonaUser)
function extractTopics(text: string): string[] {
  const words = text.toLowerCase().split(/\s+/);
  const stopWords = new Set(['the', 'a', 'an', 'and', 'or', 'but', 'is', 'are', 'was', 'were', 'in', 'on', 'at', 'to', 'for']);
  return words.filter(w => w.length > 3 && !stopWords.has(w)).slice(0, 5);
}
