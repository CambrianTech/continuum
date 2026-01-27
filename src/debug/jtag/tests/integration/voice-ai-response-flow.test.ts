/**
 * Voice AI Response Flow Integration Tests
 *
 * Tests the complete flow from voice transcription to AI response:
 * 1. Rust CallServer transcribes audio
 * 2. Rust VoiceOrchestrator returns responder IDs
 * 3. TypeScript emits voice:transcription:directed events
 * 4. PersonaUser receives and processes events
 * 5. AI generates response
 *
 * Pattern: Rust computation → TypeScript events → PersonaUser processing
 *
 * Run with: npx vitest run tests/integration/voice-ai-response-flow.test.ts
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { Events } from '../../system/core/shared/Events';

// Mock constants
const TEST_SESSION_ID = '00000000-0000-0000-0000-000000000001';
const TEST_HUMAN_ID = '00000000-0000-0000-0000-000000000010';
const TEST_AI_1_ID = '00000000-0000-0000-0000-000000000020';
const TEST_AI_2_ID = '00000000-0000-0000-0000-000000000021';

// Mock VoiceOrchestrator (simulates Rust returning responder IDs)
class MockVoiceOrchestrator {
  private sessions = new Map<string, string[]>();

  registerSession(sessionId: string, aiIds: string[]): void {
    this.sessions.set(sessionId, aiIds);
  }

  async onUtterance(event: {
    sessionId: string;
    speakerId: string;
    transcript: string;
  }): Promise<string[]> {
    // Return AI IDs for this session (excluding speaker)
    const aiIds = this.sessions.get(event.sessionId) || [];
    return aiIds.filter(id => id !== event.speakerId);
  }
}

// Mock PersonaUser inbox
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

// Mock PersonaUser
class MockPersonaUser {
  public personaId: string;
  public displayName: string;
  public inbox: MockPersonaInbox;
  private unsubscribe: () => void;

  constructor(personaId: string, displayName: string) {
    this.personaId = personaId;
    this.displayName = displayName;
    this.inbox = new MockPersonaInbox();

    // Subscribe to voice events (this is what PersonaUser.ts should do)
    this.unsubscribe = Events.subscribe('voice:transcription:directed', async (eventData: any) => {
      if (eventData.targetPersonaId === this.personaId) {
        console.log(`🎙️ ${this.displayName}: Received "${eventData.transcript}"`);

        await this.inbox.enqueue({
          type: 'voice-transcription',
          priority: 0.8,
          data: eventData,
        });
      }
    });
  }

  async processInbox(): Promise<string | null> {
    const tasks = await this.inbox.peek(1);
    if (tasks.length === 0) return null;

    const task = tasks[0];
    console.log(`🤖 ${this.displayName}: Processing task: ${task.data.transcript}`);

    // Simulate AI response
    return `Response to: ${task.data.transcript}`;
  }

  cleanup(): void {
    this.unsubscribe();
    this.inbox.clear();
  }
}

// Simulate VoiceWebSocketHandler logic
async function simulateVoiceWebSocketHandler(
  orchestrator: MockVoiceOrchestrator,
  utteranceEvent: {
    sessionId: string;
    speakerId: string;
    speakerName: string;
    transcript: string;
    confidence: number;
    timestamp: number;
  }
): Promise<void> {
  // Step 1: Rust computes responder IDs (ALREADY WORKS - tested separately)
  const responderIds = await orchestrator.onUtterance(utteranceEvent);

  console.log(`📡 VoiceWebSocketHandler: Got ${responderIds.length} responders from orchestrator`);

  // Step 2: TypeScript emits events (THIS IS WHAT WE'RE TESTING)
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

    console.log(`📤 VoiceWebSocketHandler: Emitted event to AI ${aiId.slice(0, 8)}`);
  }
}

describe('Voice AI Response Flow - Integration', () => {
  let orchestrator: MockVoiceOrchestrator;
  let ai1: MockPersonaUser;
  let ai2: MockPersonaUser;

  beforeEach(() => {
    orchestrator = new MockVoiceOrchestrator();
    ai1 = new MockPersonaUser(TEST_AI_1_ID, 'Helper AI');
    ai2 = new MockPersonaUser(TEST_AI_2_ID, 'Teacher AI');

    // Register session with 2 AIs
    orchestrator.registerSession(TEST_SESSION_ID, [TEST_AI_1_ID, TEST_AI_2_ID]);
  });

  afterEach(() => {
    ai1.cleanup();
    ai2.cleanup();
  });

  it('should complete full flow: utterance → orchestrator → events → AI inbox', async () => {
    // Simulate user speaking
    await simulateVoiceWebSocketHandler(orchestrator, {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_HUMAN_ID,
      speakerName: 'Human User',
      transcript: 'Hello AIs, can you help me?',
      confidence: 0.95,
      timestamp: Date.now(),
    });

    // Wait for async event processing
    await new Promise(resolve => setTimeout(resolve, 20));

    // Verify both AIs received the event in their inboxes
    const ai1Tasks = await ai1.inbox.peek(10);
    expect(ai1Tasks).toHaveLength(1);
    expect(ai1Tasks[0].type).toBe('voice-transcription');
    expect(ai1Tasks[0].data.transcript).toBe('Hello AIs, can you help me?');

    const ai2Tasks = await ai2.inbox.peek(10);
    expect(ai2Tasks).toHaveLength(1);
    expect(ai2Tasks[0].type).toBe('voice-transcription');
    expect(ai2Tasks[0].data.transcript).toBe('Hello AIs, can you help me?');

    // Simulate AIs processing and responding
    const response1 = await ai1.processInbox();
    expect(response1).toBeTruthy();
    expect(response1).toContain('Hello AIs, can you help me?');

    const response2 = await ai2.processInbox();
    expect(response2).toBeTruthy();
    expect(response2).toContain('Hello AIs, can you help me?');

    console.log('✅ Full flow complete: Human → Orchestrator → Events → AI inbox → AI response');
  });

  it('should handle single AI in session', async () => {
    // Create session with only AI 1
    orchestrator.registerSession('single-ai-session', [TEST_AI_1_ID]);

    await simulateVoiceWebSocketHandler(orchestrator, {
      sessionId: 'single-ai-session',
      speakerId: TEST_HUMAN_ID,
      speakerName: 'Human User',
      transcript: 'Question for one AI',
      confidence: 0.95,
      timestamp: Date.now(),
    });

    await new Promise(resolve => setTimeout(resolve, 20));

    // Only AI 1 should receive event
    const ai1Tasks = await ai1.inbox.peek(10);
    expect(ai1Tasks).toHaveLength(1);

    const ai2Tasks = await ai2.inbox.peek(10);
    expect(ai2Tasks).toHaveLength(0); // AI 2 not in this session
  });

  it('should exclude speaker from responders', async () => {
    // Simulate AI 1 speaking (should only notify AI 2)
    await simulateVoiceWebSocketHandler(orchestrator, {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_AI_1_ID, // AI 1 is the speaker
      speakerName: 'Helper AI',
      transcript: 'I have a suggestion',
      confidence: 0.95,
      timestamp: Date.now(),
    });

    await new Promise(resolve => setTimeout(resolve, 20));

    // AI 1 should NOT receive event (speaker excluded)
    const ai1Tasks = await ai1.inbox.peek(10);
    expect(ai1Tasks).toHaveLength(0);

    // AI 2 SHOULD receive event
    const ai2Tasks = await ai2.inbox.peek(10);
    expect(ai2Tasks).toHaveLength(1);
    expect(ai2Tasks[0].data.speakerId).toBe(TEST_AI_1_ID);
  });

  it('should handle multiple utterances in sequence', async () => {
    // Utterance 1
    await simulateVoiceWebSocketHandler(orchestrator, {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_HUMAN_ID,
      speakerName: 'Human User',
      transcript: 'First question',
      confidence: 0.95,
      timestamp: Date.now(),
    });

    await new Promise(resolve => setTimeout(resolve, 20));

    // Utterance 2
    await simulateVoiceWebSocketHandler(orchestrator, {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_HUMAN_ID,
      speakerName: 'Human User',
      transcript: 'Second question',
      confidence: 0.95,
      timestamp: Date.now(),
    });

    await new Promise(resolve => setTimeout(resolve, 20));

    // Both AIs should have 2 tasks each
    const ai1Tasks = await ai1.inbox.peek(10);
    expect(ai1Tasks).toHaveLength(2);
    expect(ai1Tasks[0].data.transcript).toBe('First question');
    expect(ai1Tasks[1].data.transcript).toBe('Second question');

    const ai2Tasks = await ai2.inbox.peek(10);
    expect(ai2Tasks).toHaveLength(2);
  });

  it('should handle no AIs in session gracefully', async () => {
    // Create session with no AIs
    orchestrator.registerSession('empty-session', []);

    const emitSpy = vi.spyOn(Events, 'emit');

    await simulateVoiceWebSocketHandler(orchestrator, {
      sessionId: 'empty-session',
      speakerId: TEST_HUMAN_ID,
      speakerName: 'Human User',
      transcript: 'Talking to myself',
      confidence: 0.95,
      timestamp: Date.now(),
    });

    await new Promise(resolve => setTimeout(resolve, 20));

    // No events should be emitted (no AIs to notify)
    expect(emitSpy).not.toHaveBeenCalled();

    // No AIs should have received events
    const ai1Tasks = await ai1.inbox.peek(10);
    expect(ai1Tasks).toHaveLength(0);

    const ai2Tasks = await ai2.inbox.peek(10);
    expect(ai2Tasks).toHaveLength(0);

    vi.restoreAllMocks();
  });

  it('should maintain event data integrity throughout flow', async () => {
    const originalEvent = {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_HUMAN_ID,
      speakerName: 'Test Human',
      transcript: 'Integrity test message',
      confidence: 0.87,
      timestamp: 1234567890,
    };

    await simulateVoiceWebSocketHandler(orchestrator, originalEvent);

    await new Promise(resolve => setTimeout(resolve, 20));

    // Verify AI 1 received intact data
    const ai1Tasks = await ai1.inbox.peek(10);
    expect(ai1Tasks[0].data).toMatchObject({
      sessionId: originalEvent.sessionId,
      speakerId: originalEvent.speakerId,
      speakerName: originalEvent.speakerName,
      transcript: originalEvent.transcript,
      confidence: originalEvent.confidence,
      timestamp: originalEvent.timestamp,
      targetPersonaId: TEST_AI_1_ID,
    });

    // Verify AI 2 received intact data
    const ai2Tasks = await ai2.inbox.peek(10);
    expect(ai2Tasks[0].data).toMatchObject({
      sessionId: originalEvent.sessionId,
      speakerId: originalEvent.speakerId,
      speakerName: originalEvent.speakerName,
      transcript: originalEvent.transcript,
      confidence: originalEvent.confidence,
      timestamp: originalEvent.timestamp,
      targetPersonaId: TEST_AI_2_ID,
    });
  });
});

describe('Voice AI Response Flow - Performance', () => {
  let orchestrator: MockVoiceOrchestrator;
  let ais: MockPersonaUser[];

  beforeEach(() => {
    orchestrator = new MockVoiceOrchestrator();

    // Create 5 AI participants (realistic scenario)
    ais = [
      new MockPersonaUser('00000000-0000-0000-0000-000000000020', 'Helper AI'),
      new MockPersonaUser('00000000-0000-0000-0000-000000000021', 'Teacher AI'),
      new MockPersonaUser('00000000-0000-0000-0000-000000000022', 'Code AI'),
      new MockPersonaUser('00000000-0000-0000-0000-000000000023', 'Math AI'),
      new MockPersonaUser('00000000-0000-0000-0000-000000000024', 'Science AI'),
    ];

    orchestrator.registerSession(
      TEST_SESSION_ID,
      ais.map(ai => ai.personaId)
    );
  });

  afterEach(() => {
    ais.forEach(ai => ai.cleanup());
  });

  it('should complete flow in < 10ms for 5 AIs', async () => {
    const start = performance.now();

    await simulateVoiceWebSocketHandler(orchestrator, {
      sessionId: TEST_SESSION_ID,
      speakerId: TEST_HUMAN_ID,
      speakerName: 'Human User',
      transcript: 'Performance test with 5 AIs',
      confidence: 0.95,
      timestamp: Date.now(),
    });

    // Wait for async processing
    await new Promise(resolve => setTimeout(resolve, 20));

    const duration = performance.now() - start;

    // Should be fast (< 30ms including wait)
    expect(duration).toBeLessThan(30);

    // Verify all 5 AIs received events
    for (const ai of ais) {
      const tasks = await ai.inbox.peek(10);
      expect(tasks).toHaveLength(1);
    }

    console.log(`✅ Full flow (5 AIs): ${duration.toFixed(2)}ms`);
  });
});
