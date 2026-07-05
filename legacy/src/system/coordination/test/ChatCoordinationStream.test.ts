import { describe, expect, it, vi } from 'vitest';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { ChatCoordinationStream } from '../server/ChatCoordinationStream';

describe('ChatCoordinationStream room activity decay', () => {
  it('does not decay a room immediately after activity', async () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);

    const coordinator = new ChatCoordinationStream({ enableLogging: false });
    coordinator.initialize();

    try {
      const roomId = 'room-activity' as UUID;
      coordinator.onHumanMessage(roomId);
      expect(coordinator.getTemperature(roomId)).toBeCloseTo(0.8);

      await vi.advanceTimersByTimeAsync(10_000);

      expect(coordinator.getTemperature(roomId)).toBeCloseTo(0.8);
    } finally {
      coordinator.shutdown();
      vi.useRealTimers();
    }
  });
});
