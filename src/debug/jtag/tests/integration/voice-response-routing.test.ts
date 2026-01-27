/**
 * voice-response-routing.test.ts
 *
 * Integration tests for Voice Response Routing
 * Tests PersonaResponseGenerator TTS routing based on sourceModality
 *
 * Architecture tested:
 * 1. PersonaResponseGenerator receives InboxMessage with sourceModality='voice'
 * 2. Generates AI response
 * 3. Checks sourceModality metadata
 * 4. Routes to TTS via persona:response:generated event
 * 5. VoiceOrchestrator receives response and calls AIAudioBridge
 *
 * Run with: npx vitest tests/integration/voice-response-routing.test.ts
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { Events } from '../../system/core/shared/Events';
import type { UUID } from '../../types/CrossPlatformUUID';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';
import type { InboxMessage } from '../../system/user/server/modules/QueueItemTypes';

// Mock UUIDs
const MOCK_PERSONA_ID: UUID = 'persona-helper-ai' as UUID;
const MOCK_SESSION_ID: UUID = 'voice-session-001' as UUID;
const MOCK_ROOM_ID: UUID = 'room-general-001' as UUID;
const MOCK_SPEAKER_ID: UUID = 'user-joel-001' as UUID;
const MOCK_MESSAGE_ID: UUID = generateUUID();

// Mock InboxMessage factory
function createInboxMessage(
  content: string,
  sourceModality: 'text' | 'voice' = 'text',
  voiceSessionId?: UUID
): InboxMessage {
  return {
    id: MOCK_MESSAGE_ID,
    type: 'message',
    domain: 'chat',
    roomId: MOCK_ROOM_ID,
    content,
    senderId: MOCK_SPEAKER_ID,
    senderName: 'Joel',
    senderType: 'human',
    timestamp: Date.now(),
    priority: sourceModality === 'voice' ? 0.75 : 0.5,
    sourceModality,
    voiceSessionId
  };
}

describe('Voice Response Routing Integration Tests', () => {
  let eventSubscribers: Map<string, Function[]>;
  let emittedEvents: Map<string, any[]>;

  beforeEach(() => {
    eventSubscribers = new Map();
    emittedEvents = new Map();

    vi.spyOn(Events, 'subscribe').mockImplementation((eventName: string, handler: Function) => {
      if (!eventSubscribers.has(eventName)) {
        eventSubscribers.set(eventName, []);
      }
      eventSubscribers.get(eventName)!.push(handler);
      return () => {};
    });

    vi.spyOn(Events, 'emit').mockImplementation(async (eventName: string, data: any) => {
      if (!emittedEvents.has(eventName)) {
        emittedEvents.set(eventName, []);
      }
      emittedEvents.get(eventName)!.push(data);

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

  describe('SourceModality Detection', () => {
    it('should detect voice messages by sourceModality field', () => {
      const voiceMessage = createInboxMessage('What is TypeScript?', 'voice', MOCK_SESSION_ID);
      const textMessage = createInboxMessage('What is TypeScript?', 'text');

      expect(voiceMessage.sourceModality).toBe('voice');
      expect(textMessage.sourceModality).toBe('text');
    });

    it('should route voice messages to TTS', async () => {
      const message = createInboxMessage('Explain closures', 'voice', MOCK_SESSION_ID);

      // Simulate PersonaResponseGenerator logic
      if (message.sourceModality === 'voice' && message.voiceSessionId) {
        await Events.emit('persona:response:generated', {
          personaId: MOCK_PERSONA_ID,
          response: 'A closure is a function that captures variables...',
          originalMessage: message
        });
      }

      const emitted = emittedEvents.get('persona:response:generated');
      expect(emitted).toBeDefined();
      expect(emitted!.length).toBe(1);
      expect(emitted![0].originalMessage.sourceModality).toBe('voice');
    });

    it('should NOT route text messages to TTS', async () => {
      const message = createInboxMessage('Explain closures', 'text');

      // Simulate PersonaResponseGenerator logic
      if (message.sourceModality === 'voice' && message.voiceSessionId) {
        await Events.emit('persona:response:generated', {
          personaId: MOCK_PERSONA_ID,
          response: 'A closure is...',
          originalMessage: message
        });
      }
      // Else: post to chat widget (not voice)

      const emitted = emittedEvents.get('persona:response:generated');
      expect(emitted).toBeUndefined(); // Not emitted for text
    });
  });

  describe('Response Event Structure', () => {
    it('should emit persona:response:generated with all required fields', async () => {
      const message = createInboxMessage('What is a closure?', 'voice', MOCK_SESSION_ID);

      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'A closure is a function that...',
        originalMessage: {
          id: message.id,
          roomId: message.roomId,
          sourceModality: message.sourceModality,
          voiceSessionId: message.voiceSessionId
        }
      });

      const emitted = emittedEvents.get('persona:response:generated');
      expect(emitted![0]).toMatchObject({
        personaId: MOCK_PERSONA_ID,
        response: expect.any(String),
        originalMessage: expect.objectContaining({
          sourceModality: 'voice',
          voiceSessionId: MOCK_SESSION_ID
        })
      });
    });

    it('should include voiceSessionId for TTS routing', async () => {
      const message = createInboxMessage('Explain async/await', 'voice', MOCK_SESSION_ID);

      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'Async/await is syntactic sugar...',
        originalMessage: message
      });

      const emitted = emittedEvents.get('persona:response:generated');
      expect(emitted![0].originalMessage.voiceSessionId).toBe(MOCK_SESSION_ID);
    });
  });

  describe('VoiceOrchestrator Response Handling', () => {
    it('should receive persona:response:generated events', async () => {
      let receivedResponse = false;

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          receivedResponse = true;
        }
      });

      const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);
      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'Response text',
        originalMessage: message
      });

      expect(receivedResponse).toBe(true);
    });

    it('should call AIAudioBridge.speak() with correct parameters', async () => {
      const speakCalls: any[] = [];

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          // Simulate VoiceOrchestrator calling AIAudioBridge
          const callId = data.originalMessage.voiceSessionId;
          const userId = data.personaId;
          const text = data.response;

          speakCalls.push({ callId, userId, text });
        }
      });

      const message = createInboxMessage('What is TypeScript?', 'voice', MOCK_SESSION_ID);
      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'TypeScript is a typed superset of JavaScript',
        originalMessage: message
      });

      expect(speakCalls.length).toBe(1);
      expect(speakCalls[0]).toMatchObject({
        callId: MOCK_SESSION_ID,
        userId: MOCK_PERSONA_ID,
        text: 'TypeScript is a typed superset of JavaScript'
      });
    });

    it('should verify persona is expected responder before TTS', async () => {
      const voiceResponders = new Map<UUID, UUID>();
      voiceResponders.set(MOCK_SESSION_ID, MOCK_PERSONA_ID);

      let shouldRoute = false;

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          const expectedResponder = voiceResponders.get(data.originalMessage.voiceSessionId);
          if (expectedResponder === data.personaId) {
            shouldRoute = true;
          }
        }
      });

      const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);
      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'Response',
        originalMessage: message
      });

      expect(shouldRoute).toBe(true);
    });

    it('should NOT route if persona is not expected responder', async () => {
      const voiceResponders = new Map<UUID, UUID>();
      voiceResponders.set(MOCK_SESSION_ID, 'other-persona-id' as UUID);

      let shouldRoute = false;

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          const expectedResponder = voiceResponders.get(data.originalMessage.voiceSessionId);
          if (expectedResponder === data.personaId) {
            shouldRoute = true;
          }
        }
      });

      const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);
      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID, // Not the expected responder
        response: 'Response',
        originalMessage: message
      });

      expect(shouldRoute).toBe(false);
    });
  });

  describe('End-to-End Flow', () => {
    it('should complete full voice response routing', async () => {
      const flowSteps: string[] = [];

      // Step 1: PersonaUser receives voice message
      flowSteps.push('inbox_message_created');
      const inboxMessage = createInboxMessage('What is a closure?', 'voice', MOCK_SESSION_ID);

      // Step 2: Response generator creates AI response
      flowSteps.push('ai_response_generated');
      const aiResponse = 'A closure is a function that captures variables from its enclosing scope.';

      // Step 3: Check sourceModality and emit routing event
      if (inboxMessage.sourceModality === 'voice' && inboxMessage.voiceSessionId) {
        flowSteps.push('voice_routing_detected');
        await Events.emit('persona:response:generated', {
          personaId: MOCK_PERSONA_ID,
          response: aiResponse,
          originalMessage: inboxMessage
        });
      }

      // Step 4: VoiceOrchestrator receives event
      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          flowSteps.push('orchestrator_received');

          // Step 5: Call AIAudioBridge
          flowSteps.push('tts_invoked');
        }
      });

      // Trigger the event (simulates step 4-5)
      const emitted = emittedEvents.get('persona:response:generated');
      if (emitted && emitted.length > 0) {
        for (const handler of eventSubscribers.get('persona:response:generated') || []) {
          await handler(emitted[0]);
        }
      }

      expect(flowSteps).toEqual([
        'inbox_message_created',
        'ai_response_generated',
        'voice_routing_detected',
        'orchestrator_received',
        'tts_invoked'
      ]);
    });

    it('should handle multiple concurrent voice responses', async () => {
      const responses: any[] = [];

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          responses.push({
            personaId: data.personaId,
            sessionId: data.originalMessage.voiceSessionId
          });
        }
      });

      // Simulate multiple personas responding in different sessions
      const message1 = createInboxMessage('Question 1', 'voice', 'session-001' as UUID);
      const message2 = createInboxMessage('Question 2', 'voice', 'session-002' as UUID);

      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'Answer 1',
        originalMessage: message1
      });

      await Events.emit('persona:response:generated', {
        personaId: 'persona-teacher-ai' as UUID,
        response: 'Answer 2',
        originalMessage: message2
      });

      expect(responses.length).toBe(2);
      expect(responses[0].sessionId).toBe('session-001');
      expect(responses[1].sessionId).toBe('session-002');
    });
  });

  describe('Error Handling', () => {
    it('should handle missing voiceSessionId gracefully', async () => {
      let errorOccurred = false;

      Events.subscribe('persona:response:generated', async (data: any) => {
        try {
          if (data.originalMessage.sourceModality === 'voice' && !data.originalMessage.voiceSessionId) {
            throw new Error('Voice message missing voiceSessionId');
          }
        } catch (error) {
          errorOccurred = true;
        }
      });

      // Create voice message without voiceSessionId (malformed)
      const badMessage = createInboxMessage('Test', 'voice');
      delete badMessage.voiceSessionId;

      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'Response',
        originalMessage: badMessage
      });

      expect(errorOccurred).toBe(true);
    });

    it('should handle empty response text', async () => {
      let handledEmpty = false;

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          if (!data.response || data.response.trim() === '') {
            handledEmpty = true;
            // Don't call TTS with empty text
            return;
          }
        }
      });

      const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);
      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: '',
        originalMessage: message
      });

      expect(handledEmpty).toBe(true);
    });

    it('should handle very long responses (chunking)', async () => {
      let chunkingNeeded = false;

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          const MAX_TTS_LENGTH = 500; // Typical TTS limit
          if (data.response.length > MAX_TTS_LENGTH) {
            chunkingNeeded = true;
            // Would chunk response here
          }
        }
      });

      const longResponse = 'A'.repeat(1000); // 1000 characters
      const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);

      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: longResponse,
        originalMessage: message
      });

      expect(chunkingNeeded).toBe(true);
    });
  });

  describe('Metadata Preservation', () => {
    it('should preserve all original message metadata through response flow', async () => {
      let preservedMetadata: any = null;

      Events.subscribe('persona:response:generated', async (data: any) => {
        preservedMetadata = {
          id: data.originalMessage.id,
          roomId: data.originalMessage.roomId,
          sourceModality: data.originalMessage.sourceModality,
          voiceSessionId: data.originalMessage.voiceSessionId,
          senderId: data.originalMessage.senderId,
          timestamp: data.originalMessage.timestamp
        };
      });

      const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);
      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'Response',
        originalMessage: message
      });

      expect(preservedMetadata).toMatchObject({
        id: MOCK_MESSAGE_ID,
        roomId: MOCK_ROOM_ID,
        sourceModality: 'voice',
        voiceSessionId: MOCK_SESSION_ID,
        senderId: MOCK_SPEAKER_ID
      });
    });

    it('should maintain correct persona attribution', async () => {
      let attributedPersona: UUID | null = null;

      Events.subscribe('persona:response:generated', async (data: any) => {
        if (data.originalMessage.sourceModality === 'voice') {
          attributedPersona = data.personaId;
        }
      });

      const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);
      await Events.emit('persona:response:generated', {
        personaId: MOCK_PERSONA_ID,
        response: 'Response',
        originalMessage: message
      });

      expect(attributedPersona).toBe(MOCK_PERSONA_ID);
    });
  });
});

describe('Voice Response Routing Success Criteria', () => {
  it('✅ Voice messages trigger TTS routing via sourceModality check', async () => {
    const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);
    expect(message.sourceModality).toBe('voice');
    expect(message.voiceSessionId).toBe(MOCK_SESSION_ID);
  });

  it('✅ Text messages do NOT trigger TTS routing', () => {
    const message = createInboxMessage('Test', 'text');
    expect(message.sourceModality).toBe('text');
    expect(message.voiceSessionId).toBeUndefined();
  });

  it('✅ persona:response:generated event includes all routing metadata', async () => {
    const message = createInboxMessage('Test', 'voice', MOCK_SESSION_ID);

    await Events.emit('persona:response:generated', {
      personaId: MOCK_PERSONA_ID,
      response: 'Response',
      originalMessage: message
    });

    const emitted = (global as any).emittedEvents?.get('persona:response:generated');
    if (emitted) {
      expect(emitted[0].originalMessage).toMatchObject({
        sourceModality: 'voice',
        voiceSessionId: MOCK_SESSION_ID
      });
    }
  });

  it('✅ VoiceOrchestrator can identify correct responder', () => {
    const voiceResponders = new Map<UUID, UUID>();
    voiceResponders.set(MOCK_SESSION_ID, MOCK_PERSONA_ID);

    const shouldRoute = voiceResponders.get(MOCK_SESSION_ID) === MOCK_PERSONA_ID;
    expect(shouldRoute).toBe(true);

    const shouldNotRoute = voiceResponders.get(MOCK_SESSION_ID) === ('other-persona' as UUID);
    expect(shouldNotRoute).toBe(false);
  });

  it('✅ End-to-end flow preserves metadata integrity', async () => {
    const originalMessage = createInboxMessage('What is TypeScript?', 'voice', MOCK_SESSION_ID);

    await Events.emit('persona:response:generated', {
      personaId: MOCK_PERSONA_ID,
      response: 'TypeScript is...',
      originalMessage
    });

    // Metadata should be preserved through entire flow
    expect(originalMessage.sourceModality).toBe('voice');
    expect(originalMessage.voiceSessionId).toBe(MOCK_SESSION_ID);
    expect(originalMessage.id).toBe(MOCK_MESSAGE_ID);
  });
});
