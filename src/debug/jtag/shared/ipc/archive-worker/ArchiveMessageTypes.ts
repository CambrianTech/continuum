/**
 * Message types for Archive Worker communication
 *
 * IMPORTANT: Keep in sync with Rust workers/archive/src/messages.rs
 */

// ============================================================================
// TypeScript → Rust Messages (Archive Tasks)
// ============================================================================

export type ArchiveRequest =
  | {
      command: 'archive';
      task_id: string;
      collection: string;
      source_handle: string;
      dest_handle: string;
      max_rows: number;
      batch_size: number;
    }
  | {
      command: 'ping';
    }
  | {
      command: 'status';
    };

export type ArchiveResponse =
  | {
      status: 'queued';
      task_id: string;
      queue_position: number;
    }
  | {
      status: 'complete';
      task_id: string;
      rows_archived: number;
      duration_ms: number;
    }
  | {
      status: 'error';
      task_id: string;
      error: string;
    }
  | {
      status: 'pong';
      uptime_seconds: number;
      queue_size: number;
      tasks_completed: number;
    }
  | {
      status: 'status';
      queue_size: number;
      active_tasks: number;
      completed_tasks: number;
    };

// ============================================================================
// Rust → TypeScript Messages (Command Execution)
// ============================================================================

/**
 * Request from Rust to execute a command via Commands.execute()
 * This is the BREAKTHROUGH - bidirectional communication
 */
export interface CommandExecutionRequest {
  request_id: string;
  command: string;
  params: unknown;
}

/**
 * Response from TypeScript with command result
 */
export interface CommandExecutionResponse {
  request_id: string;
  success: boolean;
  result?: unknown;
  error?: string;
}
