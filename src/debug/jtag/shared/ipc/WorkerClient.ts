/**
 * WorkerClient - Generic TypeScript Client for Rust Workers
 *
 * This is a production-ready client for communicating with Rust workers over
 * Unix domain sockets. It handles:
 * - Connection lifecycle (connect, disconnect, reconnect)
 * - Message framing (newline-delimited JSON)
 * - Request/response correlation (UUID matching)
 * - Timeout handling
 * - Error handling and recovery
 * - Type-safe generic interface
 *
 * USAGE:
 * ```typescript
 * const client = new WorkerClient<WriteLogPayload, WriteLogResult>('/tmp/logger.sock');
 * await client.connect();
 * const response = await client.send('write-log', { category: 'sql', ... });
 * ```
 *
 * NOTE: This is a generic transport layer. Worker-specific clients should extend
 * this class and provide typed convenience methods.
 */

import * as net from 'net';
import { randomUUID } from 'crypto';
import {
  WorkerRequest,
  WorkerResponse,
  isWorkerResponse,
  ErrorType
} from './WorkerMessages.js';

// ============================================================================
// Types and Interfaces
// ============================================================================

/**
 * Configuration for WorkerClient.
 */
export interface WorkerClientConfig {
  socketPath: string;
  timeout?: number;          // Request timeout in ms (default: 10000)
  reconnectDelay?: number;   // Delay before reconnect attempt in ms (default: 1000)
  maxReconnectAttempts?: number; // Max reconnect attempts (default: 3)
  userId?: string;           // Optional default userId for all requests
}

/**
 * Connection state for the worker client.
 */
export type ConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'error'
  | 'reconnecting';

/**
 * Pending request waiting for response.
 */
interface PendingRequest<TRes> {
  resolve: (response: WorkerResponse<TRes>) => void;
  reject: (error: Error) => void;
  timeoutId: NodeJS.Timeout;
}

// ============================================================================
// WorkerClient Class
// ============================================================================

/**
 * Generic client for communicating with Rust workers.
 *
 * @template TReq - Request payload type (worker-specific)
 * @template TRes - Response payload type (worker-specific)
 */
export class WorkerClient<TReq = unknown, TRes = unknown> {
  private socket: net.Socket | null = null;
  private connectionState: ConnectionState = 'disconnected';
  private buffer: string = '';
  private pendingRequests: Map<string, PendingRequest<TRes>> = new Map();
  private reconnectAttempts: number = 0;

  // Configuration
  protected readonly socketPath: string;
  protected readonly timeout: number;
  protected readonly reconnectDelay: number;
  protected readonly maxReconnectAttempts: number;
  protected readonly defaultUserId?: string;

  constructor(config: WorkerClientConfig) {
    this.socketPath = config.socketPath;
    this.timeout = config.timeout ?? 10000;
    this.reconnectDelay = config.reconnectDelay ?? 1000;
    this.maxReconnectAttempts = config.maxReconnectAttempts ?? 3;
    this.defaultUserId = config.userId;
  }

  // ============================================================================
  // Connection Lifecycle
  // ============================================================================

  /**
   * Connect to the Rust worker.
   * @throws {Error} if connection fails
   */
  async connect(): Promise<void> {
    if (this.connectionState === 'connected') {
      return; // Already connected
    }

    if (this.connectionState === 'connecting') {
      throw new Error('Connection already in progress');
    }

    this.connectionState = 'connecting';
    this.socket = net.createConnection(this.socketPath);

    return new Promise((resolve, reject) => {
      if (!this.socket) {
        reject(new Error('Socket is null'));
        return;
      }

      const connectTimeout = setTimeout(() => {
        reject(new Error(`Connection timeout after ${this.timeout}ms`));
        this.socket?.destroy();
      }, this.timeout);

      this.socket.once('connect', () => {
        clearTimeout(connectTimeout);
        this.connectionState = 'connected';
        this.reconnectAttempts = 0;
        this.setupSocketHandlers();
        resolve();
      });

      this.socket.once('error', (err) => {
        clearTimeout(connectTimeout);
        this.connectionState = 'error';
        reject(err);
      });
    });
  }

  /**
   * Disconnect from the Rust worker.
   */
  async disconnect(): Promise<void> {
    if (this.socket) {
      this.socket.end();
      this.socket = null;
    }
    this.connectionState = 'disconnected';

    // Reject all pending requests
    for (const [requestId, pending] of this.pendingRequests) {
      clearTimeout(pending.timeoutId);
      pending.reject(new Error('Client disconnected'));
      this.pendingRequests.delete(requestId);
    }
  }

  /**
   * Check if client is connected.
   */
  isConnected(): boolean {
    return this.connectionState === 'connected' && this.socket !== null;
  }

  /**
   * Get current connection state.
   */
  getConnectionState(): ConnectionState {
    return this.connectionState;
  }

  // ============================================================================
  // Message Sending
  // ============================================================================

  /**
   * Send a request to the Rust worker and wait for response.
   *
   * @param type - Message type (e.g., 'write-log')
   * @param payload - Worker-specific payload
   * @param userId - Optional userId for this request (overrides default)
   * @returns Promise resolving to worker response
   * @throws {Error} if not connected, timeout, or worker returns error
   */
  async send(
    type: string,
    payload: TReq,
    userId?: string
  ): Promise<WorkerResponse<TRes>> {
    if (!this.isConnected()) {
      throw new Error('Worker client not connected');
    }

    const request: WorkerRequest<TReq> = {
      id: randomUUID(),
      type,
      timestamp: new Date().toISOString(),
      payload,
      userId: userId ?? this.defaultUserId
    };

    return new Promise((resolve, reject) => {
      // Set up timeout
      const timeoutId = setTimeout(() => {
        this.pendingRequests.delete(request.id);
        reject(new Error(`Request timeout after ${this.timeout}ms`));
      }, this.timeout);

      // Store pending request
      this.pendingRequests.set(request.id, {
        resolve,
        reject,
        timeoutId
      });

      // Send request (newline-delimited JSON)
      const json = JSON.stringify(request) + '\n';
      this.socket!.write(json, (err) => {
        if (err) {
          clearTimeout(timeoutId);
          this.pendingRequests.delete(request.id);
          reject(err);
        }
      });
    });
  }

  // ============================================================================
  // Socket Event Handlers
  // ============================================================================

  private setupSocketHandlers(): void {
    if (!this.socket) return;

    // Handle incoming data
    this.socket.on('data', (data) => {
      this.buffer += data.toString();

      // Process complete lines (newline-delimited JSON)
      const lines = this.buffer.split('\n');
      this.buffer = lines.pop() || ''; // Keep incomplete line in buffer

      for (const line of lines) {
        if (!line.trim()) continue;

        try {
          const message = JSON.parse(line);

          if (isWorkerResponse<TRes>(message)) {
            this.handleResponse(message);
          } else {
            console.error('WorkerClient: Received non-response message:', message);
          }
        } catch (err) {
          console.error('WorkerClient: Failed to parse response:', line, err);
        }
      }
    });

    // Handle socket errors
    this.socket.on('error', (err) => {
      console.error('WorkerClient: Socket error:', err);
      this.connectionState = 'error';
      this.attemptReconnect();
    });

    // Handle socket close
    this.socket.on('close', () => {
      this.connectionState = 'disconnected';
      this.attemptReconnect();
    });
  }

  private handleResponse(response: WorkerResponse<TRes>): void {
    const pending = this.pendingRequests.get(response.requestId);
    if (!pending) {
      console.warn('WorkerClient: Received response for unknown request:', response.requestId);
      return;
    }

    clearTimeout(pending.timeoutId);
    this.pendingRequests.delete(response.requestId);

    if (response.success) {
      pending.resolve(response);
    } else {
      const error = new WorkerError(
        response.error || 'Unknown worker error',
        response.errorType || 'internal',
        response.stack
      );
      pending.reject(error);
    }
  }

  private async attemptReconnect(): Promise<void> {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.error('WorkerClient: Max reconnect attempts reached');
      return;
    }

    this.connectionState = 'reconnecting';
    this.reconnectAttempts++;

    console.log(`WorkerClient: Reconnecting (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})...`);

    await new Promise(resolve => setTimeout(resolve, this.reconnectDelay));

    try {
      await this.connect();
      console.log('WorkerClient: Reconnected successfully');
    } catch (err) {
      console.error('WorkerClient: Reconnect failed:', err);
    }
  }
}

// ============================================================================
// Custom Error Class
// ============================================================================

/**
 * Error returned by Rust worker.
 */
export class WorkerError extends Error {
  constructor(
    message: string,
    public readonly errorType: ErrorType,
    public readonly stack?: string
  ) {
    super(message);
    this.name = 'WorkerError';
  }
}
