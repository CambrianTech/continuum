/**
 * QueueItemTypes - Unified inbox queue item types
 *
 * Philosophy: All work items (messages, tasks, self-generated) share common base
 * Enables PersonaInbox to handle heterogeneous priority queue with type safety
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { TaskDomain, TaskType, TaskStatus } from '../../../data/entities/TaskEntity';

// Re-export TaskStatus for use in PersonaUser
export type { TaskStatus };

/**
 * Base interface for all queue items
 * Common fields needed for priority queue management
 */
export interface BaseQueueItem {
  id: UUID;                  // Unique identifier
  type: 'message' | 'task';  // Discriminator for type narrowing
  priority: number;          // 0.0-1.0 for queue ordering (higher = more urgent)
  timestamp: number;         // When item was created (ms since epoch)
  domain: TaskDomain;        // Which domain this work belongs to (chat, code, self, etc.)
  enqueuedAt?: number;       // When item entered the inbox queue (ms since epoch, set by inbox)
}

/**
 * Chat message queue item
 * Represents a message that needs AI response
 */
export interface InboxMessage extends BaseQueueItem {
  type: 'message';
  roomId: UUID;              // Room where message was sent
  content: string;           // Message text
  senderId: UUID;            // Who sent it
  senderName: string;        // Sender display name
  senderType: 'human' | 'persona' | 'agent' | 'system';  // Sender user type
  mentions?: boolean;        // True if persona mentioned by name

  // Voice modality tracking for response routing
  sourceModality?: 'text' | 'voice';   // Where input came from (default: 'text')
  voiceSessionId?: UUID;               // Voice call context if applicable
}

/**
 * Task queue item
 * Represents a task assigned to the persona
 * Contains essential fields from TaskEntity needed for processing
 */
export interface InboxTask extends BaseQueueItem {
  type: 'task';
  taskId: UUID;              // Reference to full TaskEntity in database
  assigneeId: UUID;          // Who should do this (should match persona ID)
  createdBy: UUID;           // Who created this task
  taskType: TaskType;        // Specific type within domain
  contextId: UUID;           // Domain-specific context (roomId, fileId, etc.)
  description: string;       // Human-readable task description
  status: TaskStatus;        // Current status (pending, in_progress, etc.)
  dueDate?: number;          // Optional deadline (ms since epoch)
  estimatedDuration?: number; // Estimated time to complete (ms)
  dependsOn?: UUID[];        // Task IDs this depends on
  blockedBy?: UUID[];        // Tasks blocking this one
  metadata?: {               // Domain-specific metadata
    messageId?: UUID;
    roomId?: UUID;
    fileId?: UUID;
    pullRequestId?: UUID;
    gameId?: UUID;
    moveNotation?: string;
    exerciseId?: UUID;
    skillName?: string;
    loraLayer?: string;
    trainingData?: unknown[];
    originalTaskId?: UUID;   // For resume-work tasks
    originalDomain?: TaskDomain;
    targetDomain?: TaskDomain;
    failureCount?: number;
    failedTaskIds?: UUID[];
  };
}

/**
 * ProcessableMessage - Typed contract for the evaluation/response pipeline.
 *
 * NOT a database entity. This replaces the `any` type previously used
 * when reconstructing messages from inbox items for the response generator.
 *
 * Every field is explicitly typed — `undefined` is impossible for required fields.
 * `sourceModality` is REQUIRED (defaults to 'text'), eliminating the class of bugs
 * where voice metadata silently becomes `undefined`.
 */
export interface ProcessableMessage {
  id: UUID;
  roomId: UUID;
  senderId: UUID;
  senderName: string;
  senderType: 'human' | 'persona' | 'agent' | 'system';
  content: { text: string };
  timestamp: number;

  // Modality — REQUIRED, never undefined
  sourceModality: 'text' | 'voice';
  voiceSessionId?: UUID;  // Present when sourceModality === 'voice'

  // Metadata for pipeline compatibility (system test detection, etc.)
  metadata?: {
    isSystemTest?: boolean;
    testType?: string;
    source?: 'user' | 'system' | 'bot' | 'webhook';
  };
}

/**
 * Convert InboxMessage to ProcessableMessage.
 * Type-safe factory — no `any` casts.
 */
export function inboxMessageToProcessable(item: InboxMessage): ProcessableMessage {
  return {
    id: item.id,
    roomId: item.roomId,
    senderId: item.senderId,
    senderName: item.senderName,
    senderType: item.senderType,
    content: { text: item.content },
    timestamp: item.timestamp,
    sourceModality: item.sourceModality ?? 'text',
    voiceSessionId: item.voiceSessionId,
  };
}

/**
 * Union type for all queue items
 * TypeScript discriminated union enables exhaustive type checking
 */
export type QueueItem = InboxMessage | InboxTask;

/**
 * Type guard: Check if item is a message
 */
export function isInboxMessage(item: QueueItem): item is InboxMessage {
  return item.type === 'message';
}

/**
 * Type guard: Check if item is a task
 */
export function isInboxTask(item: QueueItem): item is InboxTask {
  return item.type === 'task';
}

// ═══════════════════════════════════════════════════════════════════
// CHANNEL ITEM FACTORIES
// Bridge from existing data interfaces → new behavioral class hierarchy
// External code constructs InboxMessage/InboxTask; factories wrap them.
// ═══════════════════════════════════════════════════════════════════

import { BaseQueueItem as ChannelBaseQueueItem } from './channels/BaseQueueItem';
import { VoiceQueueItem } from './channels/VoiceQueueItem';
import { ChatQueueItem } from './channels/ChatQueueItem';
import { TaskQueueItem } from './channels/TaskQueueItem';

// Re-export for external use
export { ChannelBaseQueueItem, VoiceQueueItem, ChatQueueItem, TaskQueueItem };

/**
 * Convert InboxMessage → VoiceQueueItem (for voice modality messages)
 */
export function toVoiceQueueItem(msg: InboxMessage): VoiceQueueItem {
  return new VoiceQueueItem({
    id: msg.id,
    timestamp: msg.timestamp,
    enqueuedAt: msg.enqueuedAt,
    roomId: msg.roomId,
    content: msg.content,
    senderId: msg.senderId,
    senderName: msg.senderName,
    senderType: msg.senderType,
    voiceSessionId: msg.voiceSessionId!,  // Voice messages must have voiceSessionId
  });
}

/**
 * Convert InboxMessage → ChatQueueItem (for text modality messages)
 */
export function toChatQueueItem(msg: InboxMessage): ChatQueueItem {
  return new ChatQueueItem({
    id: msg.id,
    timestamp: msg.timestamp,
    enqueuedAt: msg.enqueuedAt,
    roomId: msg.roomId,
    content: msg.content,
    senderId: msg.senderId,
    senderName: msg.senderName,
    senderType: msg.senderType,
    mentions: msg.mentions ?? false,
    priority: msg.priority,
  });
}

/**
 * Convert InboxTask → TaskQueueItem
 */
export function toTaskQueueItem(task: InboxTask): TaskQueueItem {
  return new TaskQueueItem({
    id: task.id,
    timestamp: task.timestamp,
    enqueuedAt: task.enqueuedAt,
    taskId: task.taskId,
    assigneeId: task.assigneeId,
    createdBy: task.createdBy,
    taskDomain: task.domain,
    taskType: task.taskType,
    contextId: task.contextId,
    description: task.description,
    priority: task.priority,
    status: task.status,
    dueDate: task.dueDate,
    estimatedDuration: task.estimatedDuration,
    dependsOn: task.dependsOn,
    blockedBy: task.blockedBy,
    metadata: task.metadata as Record<string, unknown> | undefined,
  });
}

/**
 * Route any QueueItem to the appropriate channel item class.
 * This is the main entry point for converting legacy data interfaces
 * to behavioral class instances.
 */
export function toChannelItem(item: QueueItem): ChannelBaseQueueItem {
  if (isInboxMessage(item)) {
    // Voice messages route to VoiceQueueItem
    if (item.sourceModality === 'voice' && item.voiceSessionId) {
      return toVoiceQueueItem(item);
    }
    // Text messages route to ChatQueueItem
    return toChatQueueItem(item);
  }

  if (isInboxTask(item)) {
    return toTaskQueueItem(item);
  }

  // Exhaustive check — should never reach here with proper discriminated union
  const _exhaustive: never = item;
  throw new Error(`Unknown queue item type: ${(item as QueueItem).type}`);
}

/**
 * Convert TaskEntity to InboxTask (lightweight queue representation)
 */
export function taskEntityToInboxTask(task: {
  id: UUID;
  assigneeId: UUID;
  createdBy: UUID;
  domain: TaskDomain;
  taskType: TaskType;
  contextId: UUID;
  description: string;
  priority: number;
  status: TaskStatus;
  createdAt: Date | string;
  dueDate?: Date | string;
  estimatedDuration?: number;
  dependsOn?: UUID[];
  blockedBy?: UUID[];
  metadata?: {
    messageId?: UUID;
    roomId?: UUID;
    fileId?: UUID;
    pullRequestId?: UUID;
    gameId?: UUID;
    moveNotation?: string;
    exerciseId?: UUID;
    skillName?: string;
    loraLayer?: string;
    trainingData?: unknown[];
    originalTaskId?: UUID;
    originalDomain?: TaskDomain;
    targetDomain?: TaskDomain;
    failureCount?: number;
    failedTaskIds?: UUID[];
  };
}): InboxTask {
  // Helper to safely convert Date | string | undefined to timestamp
  const toTimestamp = (value: Date | string | undefined): number => {
    if (!value) return Date.now(); // Fallback to now if missing
    if (typeof value === 'string') return new Date(value).getTime();
    return value.getTime();
  };

  return {
    id: task.id,
    type: 'task',
    taskId: task.id,
    assigneeId: task.assigneeId,
    createdBy: task.createdBy,
    domain: task.domain,
    taskType: task.taskType,
    contextId: task.contextId,
    description: task.description,
    priority: task.priority,
    status: task.status,
    timestamp: toTimestamp(task.createdAt),
    dueDate: task.dueDate ? toTimestamp(task.dueDate) : undefined,
    estimatedDuration: task.estimatedDuration,
    dependsOn: task.dependsOn,
    blockedBy: task.blockedBy,
    metadata: task.metadata
  };
}
