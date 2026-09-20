import type { AircRealtimeEnvelope } from '@shared/generated/airc/AircRealtimeEnvelope';
import type { AircRealtimePayloadRef } from '@shared/generated/airc/AircRealtimePayloadRef';
import { ChatMessageEntity, type MessageMetadata } from '@system/data/entities/ChatMessageEntity';
import type { AircChatTranscriptInline } from '../shared/AircChatEnvelope';
import type { AircChatMirrorEvent } from './AircChatMirrorTypes';

export function mirrorEventToChatMessage(event: AircChatMirrorEvent): ChatMessageEntity | undefined {
  const inline = extractChatTranscript(event.envelope);
  if (!inline) return undefined;

  const message = new ChatMessageEntity();
  message.id = event.eventId;
  message.roomId = inline.roomId;
  message.senderId = inline.senderId;
  message.senderName = inline.senderName;
  message.senderType = inline.senderType;
  message.content = {
    text: inline.text,
    media: inline.media,
  };
  message.replyToId = inline.replyToId;
  message.status = 'sent';
  message.priority = 'normal';
  message.timestamp = new Date(inline.timestampMs);
  message.reactions = [];
  message.metadata = mergeMirrorMetadata(inline, event);
  return message;
}

function extractChatTranscript(envelope: AircRealtimeEnvelope): AircChatTranscriptInline | undefined {
  if (envelope.payload.kind !== 'existing_schema') return undefined;

  const payload = envelope.payload.payload as AircRealtimePayloadRef;
  if (payload.schema !== 'chat_transcript') return undefined;

  const inline = payload.inline;
  if (!isChatTranscriptInline(inline)) return undefined;

  return inline;
}

function isChatTranscriptInline(value: unknown): value is AircChatTranscriptInline {
  if (!value || typeof value !== 'object') return false;
  const candidate = value as Partial<AircChatTranscriptInline>;
  return candidate.kind === 'continuum.chat.message'
    && typeof candidate.messageId === 'string'
    && typeof candidate.roomId === 'string'
    && typeof candidate.senderId === 'string'
    && typeof candidate.senderName === 'string'
    && typeof candidate.text === 'string'
    && typeof candidate.timestampMs === 'number'
    && Array.isArray(candidate.media);
}

function mergeMirrorMetadata(
  inline: AircChatTranscriptInline,
  event: AircChatMirrorEvent,
): Partial<MessageMetadata> {
  const metadata: Partial<MessageMetadata> & Record<string, unknown> = {
    ...(inline.metadata ?? {}),
  };

  metadata.source = metadata.source ?? 'user';
  metadata.aircEventId = event.eventId;
  metadata.aircLamport = event.lamport;
  metadata.aircOccurredAtMs = event.occurredAtMs;
  metadata.aircEnvelopeEventId = event.envelope.eventId;
  if (event.envelope.traceId && event.envelope.traceId !== event.eventId) {
    metadata.legacyOrmId = event.envelope.traceId;
  }

  return metadata;
}
