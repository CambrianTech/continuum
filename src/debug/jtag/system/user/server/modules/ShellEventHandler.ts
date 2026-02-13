/**
 * ShellEventHandler - Routes shell events to persona inbox
 *
 * Listens for shell:* events from Rust CodeModule and converts them
 * to InboxTasks that land in the persona's queue.
 *
 * This completes the feedback loop:
 *   Persona → shell/execute → runs → events → inbox → persona reacts
 */

import { Events } from '../../../../system/core/shared/Events';
import type { InboxTask } from './QueueItemTypes';
import type { TaskDomain, TaskType } from '../../../data/entities/TaskEntity';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';
import { v4 as uuidv4 } from 'uuid';

/** Shell event payload from Rust */
interface ShellCompleteEvent {
  execution_id: string;
  command: string;
  exit_code: number;
  success: boolean;
  stdout_lines: number;
  stderr_lines: number;
  has_error: boolean;
}

interface ShellErrorEvent {
  execution_id: string;
  command: string;
  exit_code: number;
  error_preview: string;
}

interface ShellStartedEvent {
  execution_id: string;
  command: string;
}

/** Callback type for routing tasks to persona inbox */
type InboxRouter = (personaId: string, task: InboxTask) => Promise<void>;

/**
 * Initialize shell event handling for a persona.
 * Call once during persona startup.
 */
export function initShellEventHandler(personaId: string, router: InboxRouter): () => void {
  const eventPatterns = [
    `shell:${personaId}:complete`,
    `shell:${personaId}:error`,
    `shell:${personaId}:started`,
  ];

  // Subscribe to completion events
  const unsubComplete = Events.subscribe(`shell:${personaId}:complete`, async (event: ShellCompleteEvent) => {
    const task = createShellTask(personaId, event, 'shell-complete');
    await router(personaId, task);
  });

  // Subscribe to error events (higher priority)
  const unsubError = Events.subscribe(`shell:${personaId}:error`, async (event: ShellErrorEvent) => {
    const task = createErrorTask(personaId, event);
    await router(personaId, task);
  });

  // Subscribe to started events (for tracking)
  const unsubStarted = Events.subscribe(`shell:${personaId}:started`, async (event: ShellStartedEvent) => {
    // Optional: track started executions
    // Usually don't need to create a task for this, just log
    console.log(`[ShellEventHandler] ${personaId}: Started execution ${event.execution_id} - ${event.command}`);
  });

  // Return cleanup function
  return () => {
    unsubComplete();
    unsubError();
    unsubStarted();
  };
}

/**
 * Create an InboxTask from a shell completion event
 */
function createShellTask(
  personaId: string,
  event: ShellCompleteEvent,
  taskType: TaskType
): InboxTask {
  const description = event.success
    ? `Command completed: ${event.command} (${event.stdout_lines} lines output)`
    : `Command failed: ${event.command} (exit code ${event.exit_code})`;

  return {
    id: uuidv4() as UUID,
    type: 'task',
    domain: 'code' as TaskDomain,
    taskType: taskType,
    priority: event.success ? 0.4 : 0.8, // Failed commands get higher priority
    timestamp: Date.now(),
    taskId: event.execution_id as UUID,
    assigneeId: personaId as UUID,
    createdBy: 'system' as UUID,
    contextId: event.execution_id as UUID,
    description,
    status: 'pending',
    metadata: {
      command: event.command,
      exitCode: event.exit_code,
      success: event.success,
      stdoutLines: event.stdout_lines,
      stderrLines: event.stderr_lines,
    },
  };
}

/**
 * Create an InboxTask from a shell error event
 */
function createErrorTask(personaId: string, event: ShellErrorEvent): InboxTask {
  return {
    id: uuidv4() as UUID,
    type: 'task',
    domain: 'code' as TaskDomain,
    taskType: 'fix-error',
    priority: 0.9, // High priority - errors need attention
    timestamp: Date.now(),
    taskId: event.execution_id as UUID,
    assigneeId: personaId as UUID,
    createdBy: 'system' as UUID,
    contextId: event.execution_id as UUID,
    description: `Shell error (exit ${event.exit_code}): ${event.command}\n${event.error_preview}`,
    status: 'pending',
    metadata: {
      command: event.command,
      exitCode: event.exit_code,
      errorPreview: event.error_preview,
    },
  };
}

export type { ShellCompleteEvent, ShellErrorEvent, ShellStartedEvent };
