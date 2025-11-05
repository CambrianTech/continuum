/**
 * Message Factory - Easy Message Creation
 *
 * Makes it simple to create chat messages without specifying every field.
 * Inspired by Serde's easy entity creation.
 */

import { ChatMessageEntity } from '../entities/ChatMessageEntity';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { generateUUID } from '../../core/types/CrossPlatformUUID';
import { DEFAULT_USERS, DEFAULT_ROOMS } from '../domains/DefaultEntities';
import type { UserType } from '../entities/UserEntity';

export interface EasyMessageOptions {
  text: string;
  room?: UUID | 'general' | 'academy';
  sender?: UUID | 'joel' | 'claude' | 'ai';
  senderName?: string;
  senderType?: UserType; // Optional - inferred from sender alias if not provided
  status?: 'sending' | 'sent' | 'failed';
  priority?: 'low' | 'normal' | 'high' | 'urgent';
}

/**
 * Create a message easily - just provide text and optionally room/sender
 *
 * @example
 * // Simplest form - defaults to General room, Joel as sender
 * const msg = createMessage("Hello world!");
 *
 * // Specify room and sender
 * const msg = createMessage("Welcome to academy!", { room: 'academy', sender: 'claude' });
 *
 * // Full control
 * const msg = createMessage("Important!", { room: roomId, sender: userId, priority: 'high' });
 */
export function createMessage(textOrOptions: string | EasyMessageOptions, options?: Partial<EasyMessageOptions>): ChatMessageEntity {
  // Handle both signatures: createMessage("text") and createMessage({ text: "text" })
  const opts: EasyMessageOptions = typeof textOrOptions === 'string'
    ? { text: textOrOptions, ...options }
    : textOrOptions;

  const message = new ChatMessageEntity();

  // Generate ID
  message.id = generateUUID();

  // Content - just the text, attachments default to empty
  message.content = {
    text: opts.text,
    attachments: []
  };

  // Room - support aliases
  if (opts.room === 'general' || !opts.room) {
    message.roomId = DEFAULT_ROOMS.GENERAL as UUID;
  } else if (opts.room === 'academy') {
    message.roomId = DEFAULT_ROOMS.ACADEMY as UUID;
  } else {
    message.roomId = opts.room;
  }

  // Sender - support aliases and infer senderType
  if (opts.sender === 'joel' || !opts.sender) {
    message.senderId = DEFAULT_USERS.HUMAN as UUID;
    message.senderName = opts.senderName || 'Joel';
    message.senderType = opts.senderType || 'human'; // Denormalized from UserEntity
  } else if (opts.sender === 'claude' || opts.sender === 'ai') {
    message.senderId = DEFAULT_USERS.CLAUDE_CODE as UUID;
    message.senderName = opts.senderName || 'Claude Code';
    message.senderType = opts.senderType || 'agent'; // Denormalized (Claude Code is agent)
  } else {
    message.senderId = opts.sender;
    message.senderName = opts.senderName || 'User';
    message.senderType = opts.senderType || 'human'; // Default to human if not specified
  }

  // Simple defaults for everything else
  message.status = opts.status || 'sent';
  message.priority = opts.priority || 'normal';
  message.timestamp = new Date();
  message.reactions = [];

  return message;
}

/**
 * Create a message from AI agent
 */
export function createAIMessage(text: string, options?: Omit<EasyMessageOptions, 'text' | 'sender'>): ChatMessageEntity {
  return createMessage(text, { ...options, sender: 'claude' });
}

/**
 * Create a message from human
 */
export function createHumanMessage(text: string, options?: Omit<EasyMessageOptions, 'text' | 'sender'>): ChatMessageEntity {
  return createMessage(text, { ...options, sender: 'joel' });
}