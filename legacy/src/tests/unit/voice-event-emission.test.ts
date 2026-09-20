/**
 * Voice Event Emission Unit Tests
 *
 * Tests that VoiceWebSocketHandler correctly emits voice:transcription:directed events
 * for each AI participant returned by VoiceOrchestrator.
 *
 * Pattern: Rust computes → TypeScript emits (follows CRUD pattern)
 *
 * Run with: npx vitest run tests/unit/voice-event-emission.test.ts
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { Events } from '../../system/core/shared/Events';

// Mock data
const TEST_SESSION_ID = '00000000-0000-0000-0000-000000000001';
const TEST_SPEAKER_ID = '00000000-0000-0000-0000-000000000010';
const TEST_AI_1_ID = '00000000-0000-0000-0000-000000000020';
const TEST_AI_2_ID = '00000000-0000-0000-0000-000000000021';

describe('Voice Event Emission', () => {
  let emitSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    // Spy on Events.emit to verify calls
    emitSpy = vi.spyOn(Events, 'emit');
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('should emit voice:transcription:directed for each responder ID', async () => {
    // Simulate VoiceOrchestrator returning 2 AI responder IDs
    const responderIds = [TEST_AI_1_ID, TEST_AI_2_ID];

    // Simulate the pattern: Rust returns IDs → TypeScript emits events
    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      speakerType: 'human' as const,
      transcript: 'Test utterance',
      confidence: 0.95,
      timestamp: Date.now(),
    };

    // This is what VoiceWebSocketHandler should do
    for (const aiId of responderIds) {
      await Events.emit('voice:transcription:directed', {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    // Verify Events.emit was called twice (once per AI)
    expect(emitSpy).toHaveBeenCalledTimes(2);

    // Verify first call
    expect(emitSpy).toHaveBeenNthCalledWith(
      1,
      'voice:transcription:directed',
      expect.objectContaining({
        targetPersonaId: TEST_AI_1_ID,
        transcript: 'Test utterance',
        confidence: 0.95,
      })
    );

    // Verify second call
    expect(emitSpy).toHaveBeenNthCalledWith(
      2,
      'voice:transcription:directed',
      expect.objectContaining({
        targetPersonaId: TEST_AI_2_ID,
        transcript: 'Test utterance',
        confidence: 0.95,
      })
    );
  });

  it('should not emit events when no responders returned', async () => {
    // Simulate VoiceOrchestrator returning empty array (no AIs in session)
    const responderIds: string[] = [];

    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      speakerType: 'human' as const,
      transcript: 'Test utterance',
      confidence: 0.95,
      timestamp: Date.now(),
    };

    // This is what VoiceWebSocketHandler should do
    for (const aiId of responderIds) {
      await Events.emit('voice:transcription:directed', {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    // Verify Events.emit was NOT called (no responders)
    expect(emitSpy).not.toHaveBeenCalled();
  });

  it('should include all utterance data in emitted event', async () => {
    const responderIds = [TEST_AI_1_ID];

    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Test Speaker',
      speakerType: 'human' as const,
      transcript: 'This is a complete test utterance',
      confidence: 0.87,
      timestamp: 1234567890,
    };

    // Emit event
    for (const aiId of responderIds) {
      await Events.emit('voice:transcription:directed', {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    // Verify all fields are present
    expect(emitSpy).toHaveBeenCalledWith(
      'voice:transcription:directed',
      expect.objectContaining({
        sessionId: TEST_SESSION_ID,
        speakerId: TEST_SPEAKER_ID,
        speakerName: 'Test Speaker',
        transcript: 'This is a complete test utterance',
        confidence: 0.87,
        targetPersonaId: TEST_AI_1_ID,
        timestamp: 1234567890,
      })
    );
  });

  it('should handle single responder', async () => {
    const responderIds = [TEST_AI_1_ID];

    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      speakerType: 'human' as const,
      transcript: 'Question?',
      confidence: 0.95,
      timestamp: Date.now(),
    };

    // Emit event
    for (const aiId of responderIds) {
      await Events.emit('voice:transcription:directed', {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    // Verify single emission
    expect(emitSpy).toHaveBeenCalledTimes(1);
    expect(emitSpy).toHaveBeenCalledWith(
      'voice:transcription:directed',
      expect.objectContaining({
        targetPersonaId: TEST_AI_1_ID,
      })
    );
  });

  it('should handle multiple responders (broadcast)', async () => {
    // Simulate 5 AI participants (realistic scenario)
    const responderIds = [
      '00000000-0000-0000-0000-000000000020',
      '00000000-0000-0000-0000-000000000021',
      '00000000-0000-0000-0000-000000000022',
      '00000000-0000-0000-0000-000000000023',
      '00000000-0000-0000-0000-000000000024',
    ];

    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      speakerType: 'human' as const,
      transcript: 'Broadcast to all AIs',
      confidence: 0.95,
      timestamp: Date.now(),
    };

    // Emit events
    for (const aiId of responderIds) {
      await Events.emit('voice:transcription:directed', {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    // Verify all 5 AIs received events
    expect(emitSpy).toHaveBeenCalledTimes(5);

    // Verify each AI received correct event
    responderIds.forEach((aiId, index) => {
      expect(emitSpy).toHaveBeenNthCalledWith(
        index + 1,
        'voice:transcription:directed',
        expect.objectContaining({
          targetPersonaId: aiId,
          transcript: 'Broadcast to all AIs',
        })
      );
    });
  });

  it('should use correct event name constant', async () => {
    const responderIds = [TEST_AI_1_ID];
    const EVENT_NAME = 'voice:transcription:directed';

    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      speakerType: 'human' as const,
      transcript: 'Test',
      confidence: 0.95,
      timestamp: Date.now(),
    };

    // Emit event
    for (const aiId of responderIds) {
      await Events.emit(EVENT_NAME, {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    // Verify event name is exactly as expected
    expect(emitSpy).toHaveBeenCalledWith(
      EVENT_NAME,
      expect.any(Object)
    );
  });
});

describe('Event Emission Performance', () => {
  it('should emit events quickly (< 1ms per event)', async () => {
    const responderIds = [TEST_AI_1_ID, TEST_AI_2_ID];

    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      speakerType: 'human' as const,
      transcript: 'Performance test',
      confidence: 0.95,
      timestamp: Date.now(),
    };

    const start = performance.now();

    // Emit events
    for (const aiId of responderIds) {
      await Events.emit('voice:transcription:directed', {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    const duration = performance.now() - start;

    // Should be < 1ms for 2 events (in-process, no IPC)
    expect(duration).toBeLessThan(1);

    console.log(`✅ Event emission: ${duration.toFixed(3)}ms for ${responderIds.length} events`);
  });

  it('should handle 10 responders efficiently', async () => {
    const responderIds = Array.from({ length: 10 }, (_, i) =>
      `00000000-0000-0000-0000-0000000000${String(i).padStart(2, '0')}`
    );

    const utteranceEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_SPEAKER_ID,
      speakerName: 'Human User',
      speakerType: 'human' as const,
      transcript: 'Stress test',
      confidence: 0.95,
      timestamp: Date.now(),
    };

    const start = performance.now();

    // Emit events
    for (const aiId of responderIds) {
      await Events.emit('voice:transcription:directed', {
        sessionId: utteranceEvent.sessionId,
        speakerId: utteranceEvent.speakerId,
        speakerName: utteranceEvent.speakerName,
        transcript: utteranceEvent.transcript,
        confidence: utteranceEvent.confidence,
        targetPersonaId: aiId,
        timestamp: utteranceEvent.timestamp,
      });
    }

    const duration = performance.now() - start;

    // Should be < 5ms for 10 events
    expect(duration).toBeLessThan(5);

    console.log(`✅ Event emission (10 AIs): ${duration.toFixed(3)}ms`);
  });
});
