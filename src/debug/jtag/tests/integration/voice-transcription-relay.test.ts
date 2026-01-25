/**
 * Integration Test: Voice Transcription Relay Flow
 *
 * Tests the critical STEP 10: Rust → TypeScript transcription relay
 *
 * Flow:
 * 1. Set up voice call session with AI participants
 * 2. Rust continuum-core transcribes audio → sends Transcription message
 * 3. VoiceWebSocketHandler receives message → relays to VoiceOrchestrator
 * 4. VoiceOrchestrator broadcasts to all AI participants
 * 5. AIs receive voice:transcription:directed events
 */

import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import type { UUID } from '../../types/CrossPlatformUUID.js';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID.js';
import { Events } from '../../system/core/shared/Events.js';
import { Commands } from '../../system/core/shared/Commands.js';
import { getVoiceOrchestrator } from '../../system/voice/server/VoiceOrchestrator.js';
import type { UtteranceEvent } from '../../system/voice/shared/VoiceTypes.js';
import type { UserCreateParams, UserCreateResult } from '../../commands/user/create/shared/UserCreateTypes.js';

describe('Voice Transcription Relay (STEP 10)', () => {
  let capturedEvents: any[] = [];
  let testSessionId: UUID;
  let testRoomId: UUID;
  let testSpeakerId: UUID;
  let testAIIds: UUID[] = [];

  beforeAll(async () => {
    // Create test users (speaker + 2 AIs)
    testSessionId = generateUUID();
    testRoomId = generateUUID();

    // Create human speaker
    const speakerResult = await Commands.execute<UserCreateParams, UserCreateResult>('user/create', {
      uniqueId: `test-speaker-${Date.now()}`,
      displayName: 'Test Speaker',
      type: 'human'
    });
    if (!speakerResult.success || !speakerResult.entity?.id) {
      throw new Error('Failed to create test speaker');
    }
    testSpeakerId = speakerResult.entity.id as UUID;

    // Create 2 AI participants
    for (let i = 0; i < 2; i++) {
      const aiResult = await Commands.execute<UserCreateParams, UserCreateResult>('user/create', {
        uniqueId: `test-ai-${i}-${Date.now()}`,
        displayName: `Test AI ${i}`,
        type: 'persona'
      });
      if (!aiResult.success || !aiResult.entity?.id) {
        throw new Error(`Failed to create test AI ${i}`);
      }
      testAIIds.push(aiResult.entity.id as UUID);
    }

    // Register voice session with participants
    const orchestrator = getVoiceOrchestrator();
    await orchestrator.registerSession(testSessionId, testRoomId, [testSpeakerId, ...testAIIds]);

    // Subscribe to voice:transcription:directed events
    Events.subscribe('voice:transcription:directed', (event) => {
      capturedEvents.push(event);
    });
  });

  afterAll(() => {
    capturedEvents = [];
  });

  it('should relay Rust transcription to VoiceOrchestrator', async () => {
    capturedEvents = [];

    // Simulate a transcription from Rust
    const utterance: UtteranceEvent = {
      sessionId: testSessionId,  // Use registered session
      speakerId: testSpeakerId,  // Use created speaker
      speakerName: 'Test User',
      speakerType: 'human',
      transcript: 'Hello AI team, can you hear me?',
      confidence: 0.95,
      timestamp: Date.now()
    };

    // Call VoiceOrchestrator.onUtterance (what VoiceWebSocketHandler should call)
    const orchestrator = getVoiceOrchestrator();
    await orchestrator.onUtterance(utterance);

    // Verify events were emitted
    expect(capturedEvents.length).toBeGreaterThan(0);

    // Check first event has the transcription
    const firstEvent = capturedEvents[0];
    expect(firstEvent.transcript).toBe('Hello AI team, can you hear me?');
    expect(firstEvent.confidence).toBe(0.95);
    expect(firstEvent.speakerId).toBe('00000000-0000-0000-0000-000000000002');
  });

  it('should broadcast to multiple AIs (no arbiter filtering)', async () => {
    capturedEvents = [];

    const utterance: UtteranceEvent = {
      sessionId: testSessionId,
      speakerId: testSpeakerId,
      speakerName: 'Test User',
      speakerType: 'human',
      transcript: 'This is a statement, not a question',
      confidence: 0.90,
      timestamp: Date.now()
    };

    const orchestrator = getVoiceOrchestrator();
    await orchestrator.onUtterance(utterance);

    // Should broadcast even for statements (no question-only filtering)
    expect(capturedEvents.length).toBeGreaterThan(0);
    expect(capturedEvents.length).toBe(testAIIds.length); // One event per AI

    // ALL events should have the same transcript
    for (const event of capturedEvents) {
      expect(event.transcript).toBe('This is a statement, not a question');
    }
  });

  it('should handle empty transcripts gracefully', async () => {
    const utterance: UtteranceEvent = {
      sessionId: testSessionId,
      speakerId: testSpeakerId,
      speakerName: 'Test User',
      speakerType: 'human',
      transcript: '',  // Empty transcription
      confidence: 0.50,
      timestamp: Date.now()
    };

    const orchestrator = getVoiceOrchestrator();
    await expect(orchestrator.onUtterance(utterance)).resolves.not.toThrow();
  });

  it('should include targetPersonaId for each AI participant', async () => {
    capturedEvents = [];

    const utterance: UtteranceEvent = {
      sessionId: testSessionId,
      speakerId: testSpeakerId,
      speakerName: 'Test User',
      speakerType: 'human',
      transcript: 'Testing targeted events',
      confidence: 0.92,
      timestamp: Date.now()
    };

    const orchestrator = getVoiceOrchestrator();
    await orchestrator.onUtterance(utterance);

    // Should emit events for both AI participants
    expect(capturedEvents.length).toBe(testAIIds.length);

    // Each event should have a targetPersonaId matching one of our test AIs
    for (const event of capturedEvents) {
      expect(event.targetPersonaId).toBeDefined();
      expect(typeof event.targetPersonaId).toBe('string');
      expect(event.targetPersonaId.length).toBe(36); // UUID length
      expect(testAIIds).toContain(event.targetPersonaId);
    }
  });
});
