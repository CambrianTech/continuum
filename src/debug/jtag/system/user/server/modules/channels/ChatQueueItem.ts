/**
 * ChatQueueItem - Queue item for text chat messages
 *
 * Chat is the most complex channel: per-room consolidation, mention-based urgency,
 * standard RTOS aging from base class.
 *
 * Consolidation Strategy:
 *   When multiple messages from the same room are queued, they consolidate into
 *   a single work unit. The latest message becomes the "trigger" (what the AI
 *   responds to), and prior messages become context. The persona reads the full
 *   context but only needs to respond once.
 *
 * Overrides from BaseQueueItem:
 *   - isUrgent: true when persona is mentioned by name
 *   - shouldConsolidateWith: true for same-room ChatQueueItems
 *   - consolidateWith: merges messages, keeps latest as trigger
 */

import type { UUID } from '../../../../core/types/CrossPlatformUUID';
import { BaseQueueItem, ActivityDomain, type BaseQueueItemParams } from './BaseQueueItem';

export interface ChatQueueItemParams extends BaseQueueItemParams {
  roomId: UUID;
  content: string;
  senderId: UUID;
  senderName: string;
  senderType: 'human' | 'persona' | 'agent' | 'system';
  mentions: boolean;
  priority: number;
}

/**
 * A consolidated message representing prior context for the trigger message.
 * Not sent as a separate queue item — attached to the consolidated ChatQueueItem.
 */
export interface ConsolidatedContext {
  senderId: UUID;
  senderName: string;
  content: string;
  timestamp: number;
}

export class ChatQueueItem extends BaseQueueItem {
  readonly itemType = 'chat' as const;
  readonly domain = ActivityDomain.CHAT;

  readonly roomId: UUID;
  readonly content: string;
  readonly senderId: UUID;
  readonly senderName: string;
  readonly senderType: 'human' | 'persona' | 'agent' | 'system';
  readonly mentions: boolean;

  /** Prior messages consolidated into this item (empty if not consolidated) */
  readonly consolidatedContext: ConsolidatedContext[];

  private readonly _basePriority: number;

  constructor(params: ChatQueueItemParams, consolidatedContext: ConsolidatedContext[] = []) {
    super(params);
    this.roomId = params.roomId;
    this.content = params.content;
    this.senderId = params.senderId;
    this.senderName = params.senderName;
    this.senderType = params.senderType;
    this.mentions = params.mentions;
    this._basePriority = params.priority;
    this.consolidatedContext = consolidatedContext;
  }

  // Priority set by calculateMessagePriority (existing logic)
  get basePriority(): number { return this._basePriority; }

  // Urgent ONLY if persona is directly mentioned by name
  get isUrgent(): boolean { return this.mentions; }

  // Consolidate with other chat items from the SAME ROOM
  shouldConsolidateWith(other: BaseQueueItem): boolean {
    return other instanceof ChatQueueItem && other.roomId === this.roomId;
  }

  /**
   * Merge with compatible items from the same room.
   * Self = latest message (trigger). Others = prior context.
   *
   * Returns a new ChatQueueItem with consolidated context attached.
   * The AI responds to the trigger but has full room context.
   */
  consolidateWith(others: BaseQueueItem[]): ChatQueueItem {
    // Collect all messages (self + others), sort by timestamp
    const chatOthers = others.filter(
      (o): o is ChatQueueItem => o instanceof ChatQueueItem
    );

    // Build context from older messages
    const allMessages = [...chatOthers, this].sort(
      (a, b) => a.timestamp - b.timestamp
    );

    // Latest message is the trigger (self, since we're the consolidation anchor)
    const trigger = allMessages[allMessages.length - 1];
    const priorMessages = allMessages.slice(0, -1);

    // Convert prior messages to context records
    const context: ConsolidatedContext[] = [
      // Carry forward any existing consolidated context
      ...this.consolidatedContext,
      // Add prior messages as new context
      ...priorMessages.map(msg => ({
        senderId: msg.senderId,
        senderName: msg.senderName,
        content: msg.content,
        timestamp: msg.timestamp,
      })),
    ];

    // Sort context chronologically
    context.sort((a, b) => a.timestamp - b.timestamp);

    // Return new item with trigger data + consolidated context
    // Use highest priority and most recent timestamp, carry forward mentions
    const hasMentions = this.mentions || chatOthers.some(m => m.mentions);

    return new ChatQueueItem(
      {
        id: trigger.id,
        timestamp: trigger.timestamp,
        enqueuedAt: this.enqueuedAt,  // Preserve original enqueue time for aging
        roomId: trigger.roomId,
        content: trigger.content,
        senderId: trigger.senderId,
        senderName: trigger.senderName,
        senderType: trigger.senderType,
        mentions: hasMentions,
        priority: Math.max(this._basePriority, ...chatOthers.map(m => m._basePriority)),
      },
      context
    );
  }

  /** Number of messages consolidated into this item (including self) */
  get consolidatedCount(): number {
    return this.consolidatedContext.length + 1;
  }
}
