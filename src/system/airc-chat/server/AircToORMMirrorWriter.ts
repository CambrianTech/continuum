import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { mirrorEventToChatMessage } from './AircChatMirrorMapper';
import type {
  AircChatEventSource,
  AircChatMirrorCursor,
  AircChatMirrorRunResult,
  AircChatMirrorStore,
} from './AircChatMirrorTypes';

export interface AircToORMMirrorWriterOptions {
  source: AircChatEventSource;
  store: AircChatMirrorStore;
  batchLimit?: number;
}

export class AircToORMMirrorWriter {
  private readonly source: AircChatEventSource;
  private readonly store: AircChatMirrorStore;
  private readonly batchLimit: number;

  constructor(options: AircToORMMirrorWriterOptions) {
    this.source = options.source;
    this.store = options.store;
    this.batchLimit = options.batchLimit ?? 500;
  }

  async runOnce(roomId: UUID): Promise<AircChatMirrorRunResult> {
    const cursor = await this.store.loadCursor(roomId);
    const events = await this.source.fetchAfter(roomId, cursor, this.batchLimit);

    let inserted = 0;
    let duplicates = 0;
    let skipped = 0;
    let nextCursor: AircChatMirrorCursor | undefined = cursor;

    for (const event of events) {
      const message = mirrorEventToChatMessage(event);
      if (!message) {
        skipped += 1;
        nextCursor = cursorFromEvent(roomId, event.lamport, event.eventId);
        continue;
      }

      if (await this.store.hasMessage(message.id)) {
        duplicates += 1;
      } else {
        const result = await this.store.insertMessage(message);
        if (result === 'inserted') {
          inserted += 1;
        } else {
          duplicates += 1;
        }
      }

      nextCursor = cursorFromEvent(roomId, event.lamport, event.eventId);
    }

    if (nextCursor && nextCursor !== cursor) {
      await this.store.saveCursor(nextCursor);
    }

    return {
      scanned: events.length,
      inserted,
      duplicates,
      skipped,
      cursor: nextCursor,
    };
  }
}

function cursorFromEvent(roomId: UUID, lamport: number, eventId: UUID): AircChatMirrorCursor {
  return { roomId, lamport, eventId };
}
