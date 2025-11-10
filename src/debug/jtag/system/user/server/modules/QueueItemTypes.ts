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
  mentions?: boolean;        // True if persona mentioned by name
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
