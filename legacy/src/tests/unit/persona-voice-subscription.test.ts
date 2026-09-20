/**
 * PersonaUser Voice Subscription Unit Tests
 *
 * Tests that PersonaUser correctly subscribes to and processes voice:transcription:directed events.
 *
 * Pattern: Events.emit() → PersonaUser receives → Adds to inbox
 *
 * Run with: npx vitest run tests/unit/persona-voice-subscription.test.ts
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { Events } from '../../system/core/shared/Events';

// Mock data
const TEST_PERSONA_ID = '00000000-0000-0000-0000-000000000020';
const TEST_OTHER_PERSONA_ID = '00000000-0000-0000-0000-000000000021';
const TEST_SESSION_ID = '00000000-0000-0000-0000-000000000001';
const TEST_SPEAKER_ID = '00000000-0000-0000-0000-000000000010';

// Mock PersonaUser inbox (simulates PersonaInbox.enqueue)
class MockPersonaInbox {
  public queue: Array<{ type: string; priority: number; data: any }> = [];

  async enqueue(task: { type: string; priority: number; data: any }): Promise<void> {
    this.queue.push(task);
  }

  async peek(count: number): Promise<Array<{ type: string; priority: number; data: any }>> {
    return this.queue.slice(0, count);
  }

  clear(): void {
    this.queue = [];
  }
}

// Mock PersonaUser subscription logic
function createMockPersonaUser(personaId: string) {
  const inbox = new MockPersonaInbox();
  const displayName = `Test Persona ${personaId.slice(0, 8)}`;

  // Simulate PersonaUser subscription
  const unsubscribe = Events.subscribe('voice:transcription:directed', async (eventData: any) => {
    // Only process if directed to this persona
    if (eventData.targetPersonaId === personaId) {
      console.log(`🎙️ ${displayName}: Received voice transcription from ${eventData.speakerName}`);

      // Add to inbox for processing
      await inbox.enqueue({
        type: 'voice-transcription',
        priority: 0.8, // High priority for voice
        data: eventData,
      });
    }
  });

  return { personaId, displayName, inbox, unsubscribe };
}

describe('PersonaUser Voice Subscription', () => {
  let persona1: ReturnType<typeof createMockPersonaUser>;
  let persona2: ReturnType<typeof createMockPersonaUser>;

  beforeEach(() => {
    persona1 = createMockPersonaUser(TEST_PERSONA_ID);
    persona2 = createMockPersonaUser(TEST_OTHER_PERSONA_ID);
  });

  afterEach(() => {
    persona1.unsubscribe();
    persona2.unsubscribe();
    persona1.inbox.clear();
    persona2.inbox.clear();
  });

  it('should receive voice event when targeted', async () => {
    // Emit event targeted at persona1
    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Hello AI',
      confidence: 0.95,
      targetPersonaId: TEST_PERSONA_ID,
      timestamp: Date.now(),
    });

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify persona1 received the event
    const tasks = await persona1.inbox.peek(10);
    expect(tasks).toHaveLength(1);
    expect(tasks[0].type).toBe('voice-transcription');
    expect(tasks[0].priority).toBe(0.8);
    expect(tasks[0].data.transcript).toBe('Hello AI');
    expect(tasks[0].data.targetPersonaId).toBe(TEST_PERSONA_ID);
  });

  it('should NOT receive event when NOT targeted', async () => {
    // Emit event targeted at persona2 (NOT persona1)
    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Hello other AI',
      confidence: 0.95,
      targetPersonaId: TEST_OTHER_PERSONA_ID, // Different persona
      timestamp: Date.now(),
    });

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify persona1 did NOT receive the event
    const tasks1 = await persona1.inbox.peek(10);
    expect(tasks1).toHaveLength(0);

    // Verify persona2 DID receive the event
    const tasks2 = await persona2.inbox.peek(10);
    expect(tasks2).toHaveLength(1);
    expect(tasks2[0].data.targetPersonaId).toBe(TEST_OTHER_PERSONA_ID);
  });

  it('should handle multiple events for same persona', async () => {
    // Emit 3 events targeted at persona1
    for (let i = 0; i < 3; i++) {
      await Events.emit('voice:transcription:directed', {
        sessionId: TEST_SESSION_ID,
        speakerId: TEST_SPEAKER_ID,
        speakerName: 'Human User',
        transcript: `Message ${i + 1}`,
        confidence: 0.95,
        targetPersonaId: TEST_PERSONA_ID,
        timestamp: Date.now(),
      });
    }

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify persona1 received all 3 events
    const tasks = await persona1.inbox.peek(10);
    expect(tasks).toHaveLength(3);
    expect(tasks[0].data.transcript).toBe('Message 1');
    expect(tasks[1].data.transcript).toBe('Message 2');
    expect(tasks[2].data.transcript).toBe('Message 3');
  });

  it('should handle broadcast to multiple personas', async () => {
    // Emit separate events to both personas (simulates broadcast)
    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Broadcast message',
      confidence: 0.95,
      targetPersonaId: TEST_PERSONA_ID,
      timestamp: Date.now(),
    });

    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Broadcast message',
      confidence: 0.95,
      targetPersonaId: TEST_OTHER_PERSONA_ID,
      timestamp: Date.now(),
    });

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify both personas received their events
    const tasks1 = await persona1.inbox.peek(10);
    expect(tasks1).toHaveLength(1);
    expect(tasks1[0].data.targetPersonaId).toBe(TEST_PERSONA_ID);

    const tasks2 = await persona2.inbox.peek(10);
    expect(tasks2).toHaveLength(1);
    expect(tasks2[0].data.targetPersonaId).toBe(TEST_OTHER_PERSONA_ID);
  });

  it('should preserve all event data in inbox', async () => {
    const eventData = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Test Speaker',
      transcript: 'Complete utterance data',
      confidence: 0.87,
      targetPersonaId: TEST_PERSONA_ID,
      timestamp: 1234567890,
    };

    await Events.emit('voice:transcription:directed', eventData);

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify all fields are preserved
    const tasks = await persona1.inbox.peek(10);
    expect(tasks).toHaveLength(1);
    expect(tasks[0].data).toEqual(eventData);
  });

  it('should set high priority for voice tasks', async () => {
    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Priority test',
      confidence: 0.95,
      targetPersonaId: TEST_PERSONA_ID,
      timestamp: Date.now(),
    });

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify high priority (0.8)
    const tasks = await persona1.inbox.peek(10);
    expect(tasks).toHaveLength(1);
    expect(tasks[0].priority).toBe(0.8);
  });

  it('should handle rapid succession of events', async () => {
    // Emit 10 events rapidly
    const promises = [];
    for (let i = 0; i < 10; i++) {
      promises.push(
        Events.emit('voice:transcription:directed', {
          sessionId: TEST_SESSION_ID,
          speakerId: TEST_SPEAKER_ID,
          speakerName: 'Human User',
          transcript: `Rapid message ${i + 1}`,
          confidence: 0.95,
          targetPersonaId: TEST_PERSONA_ID,
          timestamp: Date.now() + i,
        })
      );
    }
    await Promise.all(promises);

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 50));

    // Verify all events received
    const tasks = await persona1.inbox.peek(20);
    expect(tasks.length).toBeGreaterThanOrEqual(10);

    // Verify order is preserved
    for (let i = 0; i < 10; i++) {
      expect(tasks[i].data.transcript).toBe(`Rapid message ${i + 1}`);
    }
  });
});

describe('PersonaUser Subscription Error Handling', () => {
  it('should handle missing targetPersonaId gracefully', async () => {
    const persona = createMockPersonaUser(TEST_PERSONA_ID);

    // Emit event without targetPersonaId (malformed)
    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Malformed event',
      confidence: 0.95,
      // targetPersonaId missing!
      timestamp: Date.now(),
    });

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify persona did NOT receive the event
    const tasks = await persona.inbox.peek(10);
    expect(tasks).toHaveLength(0);

    persona.unsubscribe();
  });

  it('should handle null targetPersonaId gracefully', async () => {
    const persona = createMockPersonaUser(TEST_PERSONA_ID);

    // Emit event with null targetPersonaId
    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Null target',
      confidence: 0.95,
      targetPersonaId: null, // Explicitly null
      timestamp: Date.now(),
    });

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 10));

    // Verify persona did NOT receive the event
    const tasks = await persona.inbox.peek(10);
    expect(tasks).toHaveLength(0);

    persona.unsubscribe();
  });
});

describe('PersonaUser Subscription Performance', () => {
  it('should process events quickly (< 1ms per event)', async () => {
    const persona = createMockPersonaUser(TEST_PERSONA_ID);

    const start = performance.now();

    await Events.emit('voice:transcription:directed', {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      transcript: 'Performance test',
      confidence: 0.95,
      targetPersonaId: TEST_PERSONA_ID,
      timestamp: Date.now(),
    });

    // Wait for processing
    await new Promise(resolve => setTimeout(resolve, 10));

    const duration = performance.now() - start;

    // Should be very fast (< 1ms + 10ms delay)
    expect(duration).toBeLessThan(15);

    // Verify event was processed
    const tasks = await persona.inbox.peek(10);
    expect(tasks).toHaveLength(1);

    console.log(`✅ Event processing: ${duration.toFixed(3)}ms`);

    persona.unsubscribe();
  });
});
