/**
 * PersonaInbox room-activity wakeup behavior.
 *
 * Regular room chat should wake cognition after a short quiet window so the
 * Rust channel queue can consolidate a burst into one conversation item.
 * Directed work still wakes immediately.
 */

import { describe, expect, it, vi } from 'vitest';
import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { PersonaInbox } from '../../modules/PersonaInbox';
import type { InboxMessage } from '../../modules/QueueItemTypes';

function message(overrides: Partial<InboxMessage> = {}): InboxMessage {
  return {
    id: 'message-1' as UUID,
    type: 'message',
    roomId: 'room-1' as UUID,
    content: 'hello',
    senderId: 'human-1' as UUID,
    senderName: 'Developer',
    senderType: 'human',
    priority: 0.6,
    timestamp: Date.now(),
    domain: 'chat' as InboxMessage['domain'],
    sourceModality: 'text',
    ...overrides,
  };
}

function inboxWithRustBridge(): PersonaInbox {
  const inbox = new PersonaInbox('persona-1' as UUID, 'Test Persona', {
    enableLogging: false,
  });

  inbox.setRustBridge({
    channelEnqueue: vi.fn().mockResolvedValue({
      routed_to: 'chat',
      status: { total_size: 1 },
    }),
  } as any);

  return inbox;
}

describe('PersonaInbox room activity debounce', () => {
  it('debounces normal chat wakeups so bursts can consolidate', async () => {
    vi.useFakeTimers();
    try {
      const inbox = inboxWithRustBridge();
      const wait = inbox.waitForWork(1000);
      let resolved = false;
      wait.then(() => {
        resolved = true;
      });

      await inbox.enqueue(message());
      await vi.advanceTimersByTimeAsync(499);
      expect(resolved).toBe(false);

      await vi.advanceTimersByTimeAsync(1);
      await expect(wait).resolves.toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it('wakes immediately for directed mentions', async () => {
    vi.useFakeTimers();
    try {
      const inbox = inboxWithRustBridge();
      const wait = inbox.waitForWork(1000);

      await inbox.enqueue(message({ mentions: true }));

      await expect(wait).resolves.toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});
