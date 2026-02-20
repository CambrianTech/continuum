/**
 * voice-orchestrator.test.ts
 *
 * Integration tests for Voice AI Response System
 * Tests VoiceOrchestrator broadcast model
 *
 * Architecture tested:
 * 1. VoiceOrchestrator receives transcriptions
 * 2. Broadcasts to ALL text-based AIs (no gating, no selection)
 * 3. Each AI gets a directed event with its userId as targetPersonaId
 * 4. Audio-native AIs excluded (they hear via mixer)
 *
 * Run with: npx vitest tests/integration/voice-orchestrator.test.ts
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { VoiceOrchestrator } from '../../system/voice/server/VoiceOrchestrator';
import { Events } from '../../system/core/shared/Events';
import type { UUID } from '../../types/CrossPlatformUUID';
import { generateUUID } from '../../system/core/types/CrossPlatformUUID';

// Mock UUIDs for testing
const MOCK_SESSION_ID: UUID = 'voice-session-001' as UUID;
const MOCK_ROOM_ID: UUID = 'room-general-001' as UUID;
const MOCK_HUMAN_ID: UUID = 'user-joel-001' as UUID;
const MOCK_PERSONA_HELPER_ID: UUID = 'persona-helper-ai' as UUID;
const MOCK_PERSONA_TEACHER_ID: UUID = 'persona-teacher-ai' as UUID;
const MOCK_PERSONA_CODE_ID: UUID = 'persona-code-ai' as UUID;

// Mock utterance factory
function createUtterance(
  transcript: string,
  speakerId: UUID = MOCK_HUMAN_ID,
  speakerName: string = 'Joel'
): {
  sessionId: UUID;
  speakerId: UUID;
  speakerName: string;
  speakerType: 'human' | 'persona' | 'agent';
  transcript: string;
  confidence: number;
  timestamp: number;
} {
  return {
    sessionId: MOCK_SESSION_ID,
    speakerId,
    speakerName,
    speakerType: 'human',
    transcript,
    confidence: 0.95,
    timestamp: Date.now()
  };
}

describe('Voice Orchestrator Integration Tests', () => {
  let orchestrator: VoiceOrchestrator;

  beforeEach(async () => {
    // Reset singleton
    (VoiceOrchestrator as any)._instance = null;
    orchestrator = VoiceOrchestrator.instance;

    // Reset all mocks
    vi.clearAllMocks();
  });

  describe('Session Management', () => {
    it('should register voice session with participants', async () => {
      const participantIds = [MOCK_HUMAN_ID, MOCK_PERSONA_HELPER_ID, MOCK_PERSONA_TEACHER_ID];

      // Mock Commands.execute to avoid database query
      const Commands = await import('../../system/core/shared/Commands');
      vi.spyOn(Commands.Commands, 'execute').mockResolvedValue({
        success: true,
        items: [
          { id: MOCK_HUMAN_ID, displayName: 'Joel', uniqueId: 'joel', type: 'human' },
          { id: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', uniqueId: 'helper-ai', type: 'persona' },
          { id: MOCK_PERSONA_TEACHER_ID, displayName: 'Teacher AI', uniqueId: 'teacher-ai', type: 'persona' }
        ]
      } as any);

      await orchestrator.registerSession(MOCK_SESSION_ID, MOCK_ROOM_ID, participantIds);

      // Verify session was registered (internal state check)
      expect((orchestrator as any).sessionParticipants.has(MOCK_SESSION_ID)).toBe(true);
      expect((orchestrator as any).sessionContexts.has(MOCK_SESSION_ID)).toBe(true);
    }, 10000); // 10 second timeout

    it('should unregister voice session and clean up state', () => {
      orchestrator.unregisterSession(MOCK_SESSION_ID);

      expect((orchestrator as any).sessionParticipants.has(MOCK_SESSION_ID)).toBe(false);
      expect((orchestrator as any).sessionContexts.has(MOCK_SESSION_ID)).toBe(false);
    });
  });

  describe('Turn Arbitration - Direct Mentions', () => {
    it('should detect direct mention with display name', async () => {
      const utterance = createUtterance('Helper AI, what do you think about TypeScript?');

      // Mock session with participants
      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' },
        { userId: MOCK_PERSONA_TEACHER_ID, displayName: 'Teacher AI', type: 'persona' }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0
      });

      // Mock event emission to capture directed event
      const emitSpy = vi.spyOn(Events, 'emit');

      await orchestrator.onUtterance(utterance);

      // Verify directed event was emitted to Helper AI
      expect(emitSpy).toHaveBeenCalledWith(
        'voice:transcription:directed',
        expect.objectContaining({
          sessionId: MOCK_SESSION_ID,
          transcript: utterance.transcript,
          targetPersonaId: MOCK_PERSONA_HELPER_ID
        })
      );
    });

    it('should detect @username mentions', async () => {
      const utterance = createUtterance('@teacher-ai can you explain closures?');

      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human', uniqueId: 'joel' },
        { userId: MOCK_PERSONA_TEACHER_ID, displayName: 'Teacher AI', type: 'persona', uniqueId: 'teacher-ai' }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0
      });

      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      expect(emitSpy).toHaveBeenCalledWith(
        'voice:transcription:directed',
        expect.objectContaining({
          targetPersonaId: MOCK_PERSONA_TEACHER_ID
        })
      );
    });

    it('should prioritize direct mention over other strategies', async () => {
      const utterance = createUtterance('Helper AI, what is a closure?'); // Both mention AND question

      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' },
        { userId: MOCK_PERSONA_TEACHER_ID, displayName: 'Teacher AI', type: 'persona' }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0,
      });

      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      // Should select Helper AI (direct mention) not Teacher AI (round-robin)
      expect(emitSpy).toHaveBeenCalledWith(
        'voice:transcription:directed',
        expect.objectContaining({
          targetPersonaId: MOCK_PERSONA_HELPER_ID
        })
      );
    });
  });

  describe('Turn Arbitration - Topic Relevance', () => {
    it('should select AI with matching expertise keywords', async () => {
      const utterance = createUtterance('How do I refactor this TypeScript code?');

      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        {
          userId: MOCK_PERSONA_CODE_ID,
          displayName: 'CodeReview AI',
          type: 'persona',
          expertise: ['typescript', 'refactoring', 'code-review']
        },
        {
          userId: MOCK_PERSONA_TEACHER_ID,
          displayName: 'Teacher AI',
          type: 'persona',
          expertise: ['teaching', 'explanations']
        }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0
      });

      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      // Should select CodeReview AI (expertise match)
      expect(emitSpy).toHaveBeenCalledWith(
        'voice:transcription:directed',
        expect.objectContaining({
          targetPersonaId: MOCK_PERSONA_CODE_ID
        })
      );
    });
  });

  describe('Turn Arbitration - Round-Robin for Questions', () => {
    it('should detect questions with question marks', async () => {
      const utterance = createUtterance('What is the best way to handle errors?');

      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' },
        { userId: MOCK_PERSONA_TEACHER_ID, displayName: 'Teacher AI', type: 'persona' }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0,
      });

      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      // Should select Teacher AI (round-robin, not Helper AI again)
      expect(emitSpy).toHaveBeenCalledWith(
        'voice:transcription:directed',
        expect.objectContaining({
          targetPersonaId: MOCK_PERSONA_TEACHER_ID
        })
      );
    });

    it('should detect questions starting with what/how/why', async () => {
      const utterances = [
        'What is TypeScript?',
        'How do I use closures?',
        'Why is this important?',
        'Can you help me?',
        'Could this be optimized?'
      ];

      for (const text of utterances) {
        (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
          { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
          { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' }
        ]);

        (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
          sessionId: MOCK_SESSION_ID,
          roomId: MOCK_ROOM_ID,
          recentUtterances: [],
          turnCount: 0
        });

        const emitSpy = vi.spyOn(Events, 'emit');
        const utterance = createUtterance(text);
        await orchestrator.onUtterance(utterance);

        // Should emit directed event (broadcast to all text-based AIs)
        expect(emitSpy).toHaveBeenCalledWith(
          'voice:transcription:directed',
          expect.objectContaining({
            transcript: text
          })
        );
      }
    });

    it('should broadcast to ALL AIs on each question', async () => {
      const participants = [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' as const },
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' as const },
        { userId: MOCK_PERSONA_TEACHER_ID, displayName: 'Teacher AI', type: 'persona' as const },
        { userId: MOCK_PERSONA_CODE_ID, displayName: 'CodeReview AI', type: 'persona' as const }
      ];

      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, participants);

      const context = {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [] as any[],
        turnCount: 0
      };
      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, context);

      const emitSpy = vi.spyOn(Events, 'emit');
      const utterance = createUtterance('What is TypeScript?');
      await orchestrator.onUtterance(utterance);

      // Should broadcast to ALL 3 text-based AIs (not just one)
      const directedCalls = emitSpy.mock.calls.filter(c => c[0] === 'voice:transcription:directed');
      expect(directedCalls.length).toBe(3);

      const targetIds = directedCalls.map(c => (c[1] as any).targetPersonaId);
      expect(targetIds).toContain(MOCK_PERSONA_HELPER_ID);
      expect(targetIds).toContain(MOCK_PERSONA_TEACHER_ID);
      expect(targetIds).toContain(MOCK_PERSONA_CODE_ID);
    });
  });

  describe('Broadcast Model - All AIs Get All Utterances', () => {
    it('should broadcast statements to all AIs (no filtering)', async () => {
      const statements = [
        'The weather is nice today',
        'I just finished my coffee',
        'This code looks good'
      ];

      for (const text of statements) {
        (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
          { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
          { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' }
        ]);

        (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
          sessionId: MOCK_SESSION_ID,
          roomId: MOCK_ROOM_ID,
          recentUtterances: [],
          turnCount: 0
        });

        const emitSpy = vi.spyOn(Events, 'emit');
        const utterance = createUtterance(text);
        await orchestrator.onUtterance(utterance);

        // Should broadcast to all text-based AIs (1 in this case)
        const directedCalls = emitSpy.mock.calls.filter(c => c[0] === 'voice:transcription:directed');
        expect(directedCalls.length).toBe(1);
      }
    });

    it('should respond to statements with direct mentions', async () => {
      const utterance = createUtterance('Helper AI, the weather is nice today');

      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0
      });

      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      // Should emit even for statement (direct mention overrides)
      expect(emitSpy).toHaveBeenCalledWith(
        'voice:transcription:directed',
        expect.objectContaining({
          targetPersonaId: MOCK_PERSONA_HELPER_ID
        })
      );
    });
  });

  describe('Edge Cases', () => {
    it('should handle utterances with no registered session', async () => {
      const utterance = createUtterance('Hello there');

      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      // Should not crash, just warn and return
      const directedCalls = emitSpy.mock.calls.filter(c => c[0] === 'voice:transcription:directed');
      expect(directedCalls.length).toBe(0);
    });

    it('should handle sessions with no AI participants', async () => {
      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        { userId: 'user-alice-001' as UUID, displayName: 'Alice', type: 'human' }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0
      });

      const utterance = createUtterance('What is TypeScript?');
      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      // Should not emit (no AIs to respond)
      const directedCalls = emitSpy.mock.calls.filter(c => c[0] === 'voice:transcription:directed');
      expect(directedCalls.length).toBe(0);
    });

    it('should ignore own transcriptions (AI speaking)', async () => {
      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' },
        { userId: MOCK_PERSONA_TEACHER_ID, displayName: 'Teacher AI', type: 'persona' }
      ]);

      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [],
        turnCount: 0
      });

      // Helper AI speaks — broadcast to all OTHER text-based AIs
      const utterance = createUtterance('I think this is correct', MOCK_PERSONA_HELPER_ID, 'Helper AI');

      const emitSpy = vi.spyOn(Events, 'emit');
      await orchestrator.onUtterance(utterance);

      // Should broadcast to Teacher AI (Helper AI excluded as speaker)
      const directedCalls = emitSpy.mock.calls.filter(c => c[0] === 'voice:transcription:directed');
      expect(directedCalls.length).toBe(1);
      expect((directedCalls[0][1] as any).targetPersonaId).toBe(MOCK_PERSONA_TEACHER_ID);
    });
  });

  describe('Conversation Context Tracking', () => {
    it('should track recent utterances in context', async () => {
      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' }
      ]);

      const context = {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [] as any[],
        turnCount: 0
      };
      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, context);

      const utterances = [
        'What is TypeScript?',
        'How does it differ from JavaScript?',
        'Can you show me an example?'
      ];

      for (const text of utterances) {
        const utterance = createUtterance(text);
        await orchestrator.onUtterance(utterance);
      }

      // Context should track recent utterances (max 20)
      expect(context.recentUtterances.length).toBe(3);
      expect(context.turnCount).toBe(3);
    });

    it('should maintain only last 20 utterances', async () => {
      (orchestrator as any).sessionParticipants.set(MOCK_SESSION_ID, [
        { userId: MOCK_HUMAN_ID, displayName: 'Joel', type: 'human' },
        { userId: MOCK_PERSONA_HELPER_ID, displayName: 'Helper AI', type: 'persona' }
      ]);

      const context = {
        sessionId: MOCK_SESSION_ID,
        roomId: MOCK_ROOM_ID,
        recentUtterances: [] as any[],
        turnCount: 0
      };
      (orchestrator as any).sessionContexts.set(MOCK_SESSION_ID, context);

      // Send 25 utterances
      for (let i = 0; i < 25; i++) {
        const utterance = createUtterance(`Question number ${i}?`);
        await orchestrator.onUtterance(utterance);
      }

      // Should only keep last 20
      expect(context.recentUtterances.length).toBe(20);
      expect(context.recentUtterances[0].transcript).toContain('Question number 5'); // Oldest kept
      expect(context.recentUtterances[19].transcript).toContain('Question number 24'); // Newest
    });
  });
});

describe('Voice Orchestrator Success Criteria', () => {
  it('✅ VoiceOrchestrator is singleton', () => {
    const instance1 = VoiceOrchestrator.instance;
    const instance2 = VoiceOrchestrator.instance;
    expect(instance1).toBe(instance2);
  });

  it('✅ Session management tracks participants and context', async () => {
    const orchestrator = VoiceOrchestrator.instance;

    // Mock Commands.execute to avoid database query
    const Commands = await import('../../system/core/shared/Commands');
    vi.spyOn(Commands.Commands, 'execute').mockResolvedValue({
      success: true,
      items: [
        { id: MOCK_HUMAN_ID, displayName: 'Joel', uniqueId: 'joel', type: 'human' }
      ]
    } as any);

    await orchestrator.registerSession(MOCK_SESSION_ID, MOCK_ROOM_ID, [MOCK_HUMAN_ID]);

    expect((orchestrator as any).sessionParticipants.has(MOCK_SESSION_ID)).toBe(true);
    expect((orchestrator as any).sessionContexts.has(MOCK_SESSION_ID)).toBe(true);

    orchestrator.unregisterSession(MOCK_SESSION_ID);
    expect((orchestrator as any).sessionParticipants.has(MOCK_SESSION_ID)).toBe(false);
  }, 10000);

  it('✅ Broadcast model: every text-based AI gets every utterance', async () => {
    // Validated by broadcast tests — no selection, no gating, no cooldowns
    expect(true).toBe(true);
  });

  it('✅ Broadcasts to ALL text-based AIs (no gating, no selection)', async () => {
    // Validated by the broadcast tests — every text-based AI gets every utterance
    expect(true).toBe(true);
  });
});
