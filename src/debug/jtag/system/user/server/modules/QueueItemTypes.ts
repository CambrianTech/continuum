/**
 * QueueItemTypes - Unified inbox queue item types
 *
 * Philosophy: All work items (messages, tasks, self-generated) share common base
 * Enables PersonaInbox to handle heterogeneous priority queue with type safety
 */

import type { UUID } from '../../../core/types/CrossPlatformUUID';
import type { TaskDomain, TaskType, TaskStatus } from '../../../data/entities/TaskEntity';
import type { ChannelEnqueueRequest } from '../../../../shared/generated';

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
    // Shell execution metadata (from CodeModule events)
    command?: string;        // Shell command that was executed
    exitCode?: number;       // Exit code from shell
    success?: boolean;       // Whether command succeeded
    stdoutLines?: number;    // Lines of stdout output
    stderrLines?: number;    // Lines of stderr output
    errorPreview?: string;   // Preview of error message (first ~100 chars)
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

/**
 * Convert a Rust service cycle JSON item back to TS QueueItem.
 *
 * Rust to_json() produces camelCase items with `type: "voice"|"chat"|"task"`.
 * This maps them to TS discriminated union: `type: "message"|"task"`.
 *
 * Returns null if the JSON is invalid or has an unknown type.
 */
export function fromRustServiceItem(json: Record<string, unknown>): QueueItem | null {
  const itemType = json.type as string;

  if (itemType === 'voice' || itemType === 'chat') {
    // Map Rust voice/chat → TS InboxMessage
    const msg: InboxMessage = {
      id: json.id as UUID,
      type: 'message',
      roomId: json.roomId as UUID,
      content: json.content as string,
      senderId: json.senderId as UUID,
      senderName: json.senderName as string,
      senderType: json.senderType as InboxMessage['senderType'],
      mentions: (json.mentions as boolean) ?? false,
      timestamp: json.timestamp as number,
      priority: json.priority as number,
      domain: 'chat' as TaskDomain,
      enqueuedAt: json.timestamp as number,
      sourceModality: itemType === 'voice' ? 'voice' : 'text',
      voiceSessionId: json.voiceSessionId as UUID | undefined,
    };
    return msg;
  }

  if (itemType === 'code') {
    // Map Rust CodeQueueItem → TS InboxTask with domain='code'
    const task: InboxTask = {
      id: json.id as UUID,
      type: 'task',
      taskId: json.id as UUID,
      assigneeId: json.persona_id as UUID ?? json.personaId as UUID,
      createdBy: json.persona_id as UUID ?? json.personaId as UUID,
      domain: 'code' as TaskDomain,
      taskType: (json.is_review ?? json.isReview) ? 'review-code' as TaskType : 'write-feature' as TaskType,
      contextId: json.room_id as UUID ?? json.roomId as UUID,
      description: json.task_description as string ?? json.taskDescription as string ?? '',
      priority: json.priority as number,
      status: 'pending' as TaskStatus,
      timestamp: json.timestamp as number,
      enqueuedAt: json.timestamp as number,
      metadata: {
        roomId: json.room_id as UUID ?? json.roomId as UUID,
      },
    };
    return task;
  }

  if (itemType === 'task') {
    const task: InboxTask = {
      id: json.id as UUID,
      type: 'task',
      taskId: json.taskId as UUID,
      assigneeId: json.assigneeId as UUID,
      createdBy: json.createdBy as UUID,
      domain: json.taskDomain as TaskDomain,
      taskType: json.taskType as TaskType,
      contextId: json.contextId as UUID,
      description: json.description as string,
      priority: json.priority as number,
      status: json.status as TaskStatus,
      timestamp: json.timestamp as number,
      enqueuedAt: json.timestamp as number,
      dueDate: json.dueDate != null ? Number(json.dueDate) : undefined,
      estimatedDuration: json.estimatedDuration != null ? Number(json.estimatedDuration) : undefined,
      dependsOn: (json.dependsOn as UUID[]) ?? [],
      blockedBy: (json.blockedBy as UUID[]) ?? [],
    };
    return task;
  }

  return null;
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
  // NOTE: Rust ORM returns dates as ISO strings (e.g., "2026-02-07T18:17:56.886Z")
  const toTimestamp = (value: Date | string | undefined): number => {
    if (!value) return Date.now(); // Fallback to now if missing
    if (typeof value === 'string') {
      const parsed = new Date(value).getTime();
      return isNaN(parsed) ? Date.now() : parsed;
    }
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

/**
 * Convert QueueItem to ChannelEnqueueRequest for Rust IPC.
 * Maps TS queue items to the discriminated union expected by Rust's channel system.
 */
export function toChannelEnqueueRequest(item: QueueItem): ChannelEnqueueRequest {
  if (isInboxMessage(item)) {
    // Voice messages
    if (item.sourceModality === 'voice' && item.voiceSessionId) {
      return {
        item_type: 'voice',
        id: item.id,
        room_id: item.roomId,
        content: item.content,
        sender_id: item.senderId,
        sender_name: item.senderName,
        sender_type: item.senderType,
        voice_session_id: item.voiceSessionId,
        timestamp: item.timestamp,
        priority: item.priority,
      };
    }

    // Chat messages
    return {
      item_type: 'chat',
      id: item.id,
      room_id: item.roomId,
      content: item.content,
      sender_id: item.senderId,
      sender_name: item.senderName,
      sender_type: item.senderType,
      mentions: item.mentions ?? false,
      timestamp: item.timestamp,
      priority: item.priority,
    };
  }

  if (isInboxTask(item)) {
    return {
      item_type: 'task',
      id: item.id,
      task_id: item.taskId,
      assignee_id: item.assigneeId,
      created_by: item.createdBy,
      task_domain: item.domain,
      task_type: item.taskType,
      context_id: item.contextId,
      description: item.description,
      priority: item.priority,
      status: item.status,
      timestamp: item.timestamp,
      due_date: item.dueDate != null ? BigInt(item.dueDate) : null,
      estimated_duration: item.estimatedDuration != null ? BigInt(item.estimatedDuration) : null,
      depends_on: item.dependsOn ?? [],
      blocked_by: item.blockedBy ?? [],
    };
  }

  const _exhaustive: never = item;
  throw new Error(`Unknown queue item type: ${(item as QueueItem).type}`);
}
