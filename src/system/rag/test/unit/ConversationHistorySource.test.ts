import { describe, expect, it } from 'vitest';
import { detectConversationHistoryPoison } from '../../sources/conversationHistoryPoison';

describe('ConversationHistorySource context poison detection', () => {
  it('filters persona meta-summary echoes from future RAG context', () => {
    const poisoned = 'I received a message from Helper AI: "Teacher AI: Yes, I can confirm seeing this startup smoke test in the General room." This indicates that Teacher AI successfully acknowledged and responded to the startup smoke test message as expected. The key pattern here is the successful completion of a multi-step communication sequence.';

    expect(detectConversationHistoryPoison(poisoned)).toBe('meta-summary-echo');
  });

  it('keeps ordinary user and persona messages', () => {
    expect(detectConversationHistoryPoison('tacos, tell me all you know')).toBeNull();
    expect(detectConversationHistoryPoison('Helper AI: I can see this startup smoke test in the General room.')).toBeNull();
    expect(detectConversationHistoryPoison('I received your startup smoke test and can respond as Helper AI.')).toBeNull();
  });

  it('still filters fabricated multi-speaker transcripts', () => {
    const fabricated = [
      'Teacher AI: I think we should test the room.',
      'Helper AI: Agreed, I can see the room.',
      'Teacher AI: Please confirm the model route.',
      'Helper AI: Confirmed, routing is local.'
    ].join('\n');

    expect(detectConversationHistoryPoison(fabricated)).toBe('fabricated-conversation');
  });
});
