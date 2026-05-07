import { describe, expect, it } from 'vitest';
import { ChatCoordinationStream, type ChatThought } from '../../system/coordination/server/ChatCoordinationStream';
import type { UUID } from '../../system/core/types/CrossPlatformUUID';

function thought(personaId: string, confidence: number, messageId: string = 'message-1'): ChatThought {
  return {
    personaId: personaId as UUID,
    personaName: personaId,
    type: 'claiming',
    confidence,
    reasoning: 'unit-test claim',
    timestamp: Date.now(),
    messageId,
    roomId: '00000000-0000-4000-8000-000000000001' as UUID,
  };
}

describe('ChatCoordinationStream', () => {
  it('grants only the configured responder count for a chat turn', async () => {
    const roomId = '00000000-0000-4000-8000-000000000001' as UUID;
    const coordinator = new ChatCoordinationStream({
      maxResponders: 1,
      intentionWindowMs: 10,
      enableLogging: false,
    });

    await coordinator.broadcastChatThought('message-1', roomId, thought('00000000-0000-4000-8000-000000000011', 0.6));
    await coordinator.broadcastChatThought('message-1', roomId, thought('00000000-0000-4000-8000-000000000012', 0.9));

    const decision = await coordinator.waitForChatDecision('message-1', 100);
    coordinator.shutdown();

    expect(decision?.granted).toEqual(['00000000-0000-4000-8000-000000000012']);
    expect(decision?.denied).toContain('00000000-0000-4000-8000-000000000011');
  });

  it('grants multiple responders by configured confidence order', async () => {
    const roomId = '00000000-0000-4000-8000-000000000001' as UUID;
    const coordinator = new ChatCoordinationStream({
      maxResponders: 2,
      intentionWindowMs: 10,
      enableLogging: false,
    });

    await coordinator.broadcastChatThought('message-2', roomId, thought('00000000-0000-4000-8000-000000000021', 0.4, 'message-2'));
    await coordinator.broadcastChatThought('message-2', roomId, thought('00000000-0000-4000-8000-000000000022', 0.95, 'message-2'));
    await coordinator.broadcastChatThought('message-2', roomId, thought('00000000-0000-4000-8000-000000000023', 0.8, 'message-2'));

    const decision = await coordinator.waitForChatDecision('message-2', 100);
    coordinator.shutdown();

    expect(decision?.granted).toEqual([
      '00000000-0000-4000-8000-000000000022',
      '00000000-0000-4000-8000-000000000023',
    ]);
    expect(decision?.denied).toEqual(['00000000-0000-4000-8000-000000000021']);
  });
});
