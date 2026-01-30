/**
 * VoiceQueueItem - Queue item for voice/audio messages
 *
 * Voice is the simplest channel: always urgent, never consolidates, never kicked.
 * Every utterance is unique and time-critical. FIFO ordering within the channel.
 *
 * Overrides from BaseQueueItem:
 *   - basePriority: 1.0 (max)
 *   - maxAgingBoost: 0 (no aging needed, already max priority)
 *   - isUrgent: true (always bypasses scheduler)
 *   - canBeKicked: false (never dropped)
 *   - kickResistance: Infinity (absolute protection)
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { BaseQueueItem, ActivityDomain, type BaseQueueItemParams } from './BaseQueueItem';

export interface VoiceQueueItemParams extends BaseQueueItemParams {
  roomId: UUID;
  content: string;
  senderId: UUID;
  senderName: string;
  senderType: 'human' | 'persona' | 'agent' | 'system';
  voiceSessionId: UUID;
}

export class VoiceQueueItem extends BaseQueueItem {
  readonly itemType = 'voice' as const;
  readonly domain = ActivityDomain.AUDIO;

  readonly roomId: UUID;
  readonly content: string;
  readonly senderId: UUID;
  readonly senderName: string;
  readonly senderType: 'human' | 'persona' | 'agent' | 'system';
  readonly voiceSessionId: UUID;

  constructor(params: VoiceQueueItemParams) {
    super(params);
    this.roomId = params.roomId;
    this.content = params.content;
    this.senderId = params.senderId;
    this.senderName = params.senderName;
    this.senderType = params.senderType;
    this.voiceSessionId = params.voiceSessionId;
  }

  // Voice is always max priority — no aging needed
  get basePriority(): number { return 1.0; }
  protected get maxAgingBoost(): number { return 0; }

  // Voice is ALWAYS urgent — bypasses cognitive scheduler
  get isUrgent(): boolean { return true; }

  // Voice NEVER consolidates — every utterance is unique and time-sensitive
  shouldConsolidateWith(): boolean { return false; }

  // Voice NEVER gets kicked — dropping voice mid-conversation is unacceptable
  get canBeKicked(): boolean { return false; }
  get kickResistance(): number { return Infinity; }
}
