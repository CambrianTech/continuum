import type { AircRealtimeEnvelope } from '@shared/generated/airc/AircRealtimeEnvelope';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { ChatMessageEntity } from '@system/data/entities/ChatMessageEntity';

export interface AircChatMirrorCursor {
  roomId: UUID;
  lamport: number;
  eventId: UUID;
}

export interface AircChatMirrorEvent {
  eventId: UUID;
  lamport: number;
  occurredAtMs: number;
  envelope: AircRealtimeEnvelope;
}

export interface AircChatEventSource {
  fetchAfter(
    roomId: UUID,
    cursor: AircChatMirrorCursor | undefined,
    limit: number,
  ): Promise<readonly AircChatMirrorEvent[]>;
}

export type AircChatMirrorInsertResult = 'inserted' | 'duplicate';

export interface AircChatMirrorStore {
  loadCursor(roomId: UUID): Promise<AircChatMirrorCursor | undefined>;
  saveCursor(cursor: AircChatMirrorCursor): Promise<void>;
  hasMessage(messageId: UUID): Promise<boolean>;
  insertMessage(message: ChatMessageEntity): Promise<AircChatMirrorInsertResult>;
}

export interface AircChatMirrorRunResult {
  scanned: number;
  inserted: number;
  duplicates: number;
  skipped: number;
  cursor?: AircChatMirrorCursor;
}
