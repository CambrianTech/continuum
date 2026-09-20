import type { AircRealtimeEnvelope } from '@shared/generated/airc/AircRealtimeEnvelope';
import type { AircRealtimePayloadRef } from '@shared/generated/airc/AircRealtimePayloadRef';
import type { ChatMessageEntity, MediaItem } from '@system/data/entities/ChatMessageEntity';
import type { UUID } from '@system/core/types/CrossPlatformUUID';
import { generateUUID } from '@system/core/types/CrossPlatformUUID';

export const AIRC_CHAT_SCHEMA_VERSION = 'continuum.chat.v1' as const;

export interface AircChatEnvelopeInput {
  roomName: string;
  storedMessage: ChatMessageEntity;
}

export interface AircChatTranscriptInline {
  kind: 'continuum.chat.message';
  schemaVersion: typeof AIRC_CHAT_SCHEMA_VERSION;
  messageId: UUID;
  roomId: UUID;
  roomName: string;
  senderId: UUID;
  senderName: string;
  senderType: ChatMessageEntity['senderType'];
  text: string;
  media: AircChatMediaRef[];
  replyToId?: UUID;
  metadata?: Record<string, unknown>;
  timestampMs: number;
}

export interface AircChatMediaRef {
  id?: string;
  type: MediaItem['type'];
  url?: string;
  blobHash?: string;
  mimeType?: string;
  filename?: string;
  size?: number;
  alt?: string;
  description?: string;
  title?: string;
  width?: number;
  height?: number;
  duration?: number;
  thumbnailUrl?: string;
}

export function buildAircChatEnvelope(input: AircChatEnvelopeInput): AircRealtimeEnvelope {
  const inline = buildInlineTranscript(input);
  const payload: AircRealtimePayloadRef = {
    schema: 'chat_transcript',
    schemaVersion: AIRC_CHAT_SCHEMA_VERSION,
    inline,
  };

  return {
    eventId: generateUUID(),
    roomId: input.storedMessage.roomId,
    sourceId: input.storedMessage.senderId,
    createdAtMs: BigInt(inline.timestampMs),
    delivery: 'durable',
    payload: {
      kind: 'existing_schema',
      payload,
    },
    traceId: input.storedMessage.id,
  };
}

export function buildInlineTranscript(input: AircChatEnvelopeInput): AircChatTranscriptInline {
  const { storedMessage } = input;
  return {
    kind: 'continuum.chat.message',
    schemaVersion: AIRC_CHAT_SCHEMA_VERSION,
    messageId: storedMessage.id as UUID,
    roomId: storedMessage.roomId,
    roomName: input.roomName,
    senderId: storedMessage.senderId,
    senderName: storedMessage.senderName,
    senderType: storedMessage.senderType,
    text: storedMessage.content.text,
    media: (storedMessage.content.media ?? []).map(toAircMediaRef),
    replyToId: storedMessage.replyToId,
    metadata: sanitizeMetadata(storedMessage.metadata),
    timestampMs: storedMessage.timestamp.getTime(),
  };
}

export function serializeAircRealtimeEnvelope(envelope: AircRealtimeEnvelope): string {
  return JSON.stringify(envelope, (_key, value) =>
    typeof value === 'bigint' ? value.toString() : value,
  );
}

function toAircMediaRef(media: MediaItem): AircChatMediaRef {
  const {
    id,
    type,
    url,
    blobHash,
    mimeType,
    filename,
    size,
    alt,
    description,
    title,
    width,
    height,
    duration,
    thumbnailUrl,
  } = media;
  return removeUndefined({
    id,
    type,
    url,
    blobHash,
    mimeType,
    filename,
    size,
    alt,
    description,
    title,
    width,
    height,
    duration,
    thumbnailUrl,
  });
}

function sanitizeMetadata(metadata: ChatMessageEntity['metadata']): Record<string, unknown> | undefined {
  if (!metadata) return undefined;
  const rest = { ...metadata };
  delete rest.editHistory;
  delete rest.deliveryReceipts;
  return removeUndefined(rest);
}

function removeUndefined<T extends Record<string, unknown>>(value: T): T {
  return Object.fromEntries(
    Object.entries(value).filter((entry): entry is [string, unknown] => entry[1] !== undefined),
  ) as T;
}
