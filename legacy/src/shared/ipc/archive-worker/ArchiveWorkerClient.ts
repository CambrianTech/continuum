/**
 * ArchiveWorkerClient - TypeScript client for Archive Rust Worker
 *
 * BREAKTHROUGH: Bidirectional communication between TypeScript and Rust
 * - TypeScript → Rust: Queue archive tasks
 * - Rust → TypeScript: Execute Commands.execute() for data operations
 *
 * This proves Rust workers can be FIRST-CLASS CITIZENS in the JTAG system.
 */

import { WorkerClient, WorkerClientConfig } from '../WorkerClient.js';
import type { ArchiveRequest, ArchiveResponse } from './ArchiveMessageTypes.js';

/**
 * Archive task parameters
 */
export interface ArchiveTaskParams {
  taskId: string;
  collection: string;
  sourceHandle: string;
  destHandle: string;
  maxRows: number;
  batchSize: number;
}

/**
 * Type-safe client for Archive Rust worker
 */
export class ArchiveWorkerClient extends WorkerClient<ArchiveRequest, ArchiveResponse> {
  constructor(config: WorkerClientConfig | string) {
    const fullConfig: WorkerClientConfig =
      typeof config === 'string'
        ? { socketPath: config }
        : config;

    super(fullConfig);
  }

  /**
   * Queue an archive task
   *
   * @param params - Archive task parameters
   * @returns Promise resolving to queued response with position
   */
  async queueArchive(params: ArchiveTaskParams): Promise<ArchiveResponse> {
    const request: ArchiveRequest = {
      command: 'archive',
      task_id: params.taskId,
      collection: params.collection,
      source_handle: params.sourceHandle,
      dest_handle: params.destHandle,
      max_rows: params.maxRows,
      batch_size: params.batchSize
    };

    const response = await this.send('archive', request);
    return response.payload as ArchiveResponse;
  }

  /**
   * Ping the worker to check health
   */
  async ping(): Promise<ArchiveResponse> {
    const request: ArchiveRequest = {
      command: 'ping'
    };

    const response = await this.send('ping', request);
    return response.payload as ArchiveResponse;
  }

  /**
   * Get queue status
   */
  async getStatus(): Promise<ArchiveResponse> {
    const request: ArchiveRequest = {
      command: 'status'
    };

    const response = await this.send('status', request);
    return response.payload as ArchiveResponse;
  }
}
