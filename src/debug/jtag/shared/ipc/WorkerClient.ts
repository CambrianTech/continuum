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
import * as fs from 'fs';
import { randomUUID } from 'crypto';
import {
  WorkerRequest,
  WorkerResponse,
  isWorkerResponse,
  ErrorType
} from './WorkerMessages.js';

// DEBUG LOGGING - COMPREHENSIVE
const DEBUG_LOG = '/tmp/worker-client-debug.log';
let logSession = 0;

function debugLog(msg: string): void {
  const timestamp = new Date().toISOString();
  const pid = process.pid;
  fs.appendFileSync(DEBUG_LOG, `[${timestamp}] [PID:${pid}] [Session:${logSession}] ${msg}\n`);
}

// Log session start on module load
debugLog('='.repeat(80));
debugLog(`WorkerClient MODULE LOADED - Process started at ${new Date().toISOString()}`);
debugLog(`Process PID: ${process.pid}`);
debugLog('='.repeat(80));

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
  maxQueueSize?: number;     // Max buffered messages when disconnected (default: 1000)
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

/**
 * Queued message waiting for connection.
 */
interface QueuedMessage<TReq> {
  type: string;
  payload: TReq;
  userId?: string;
  resolve: (response: WorkerResponse<any>) => void;
  reject: (error: Error) => void;
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
  private messageQueue: QueuedMessage<TReq>[] = [];

  // Configuration
  protected readonly socketPath: string;
  protected readonly timeout: number;
  protected readonly reconnectDelay: number;
  protected readonly maxReconnectAttempts: number;
  protected readonly defaultUserId?: string;
  protected readonly maxQueueSize: number;

  constructor(config: WorkerClientConfig) {
    logSession++;
    debugLog(`>>> CONSTRUCTOR START (session ${logSession})`);
    debugLog(`Socket path: ${config.socketPath}`);
    debugLog(`Timeout: ${config.timeout ?? 10000}ms`);

    this.socketPath = config.socketPath;
    this.timeout = config.timeout ?? 10000;
    this.reconnectDelay = config.reconnectDelay ?? 1000;
    this.maxReconnectAttempts = config.maxReconnectAttempts ?? 3;
    this.defaultUserId = config.userId;
    this.maxQueueSize = config.maxQueueSize ?? 1000;

    debugLog(`<<< CONSTRUCTOR END`);
  }

  // ============================================================================
  // Connection Lifecycle
  // ============================================================================

  /**
   * Connect to the Rust worker.
   * @throws {Error} if connection fails
   */
  async connect(): Promise<void> {
    debugLog(`connect() called - current state: ${this.connectionState}`);

    if (this.connectionState === 'connected') {
      debugLog('Already connected, returning');
      return; // Already connected
    }

    if (this.connectionState === 'connecting') {
      debugLog('Connection already in progress');
      throw new Error('Connection already in progress');
    }

    debugLog(`Creating connection to ${this.socketPath}`);
    this.connectionState = 'connecting';
    this.socket = net.createConnection(this.socketPath);

    return new Promise((resolve, reject) => {
      if (!this.socket) {
        debugLog('Socket is null!');
        reject(new Error('Socket is null'));
        return;
      }

      const connectTimeout = setTimeout(() => {
        debugLog('Connection timeout!');
        reject(new Error(`Connection timeout after ${this.timeout}ms`));
        this.socket?.destroy();
      }, this.timeout);

      this.socket.once('connect', () => {
        debugLog('Socket connected event fired');
        clearTimeout(connectTimeout);
        this.connectionState = 'connected';
        this.reconnectAttempts = 0;
        this.setupSocketHandlers();
        debugLog('setupSocketHandlers() complete');

        // Flush queued messages
        this.flushQueue();

        resolve();
      });

      this.socket.once('error', (err) => {
        debugLog(`Socket error during connect: ${err.message}`);
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
    debugLog(`>>> DISCONNECT called - state: ${this.connectionState}`);
    if (this.socket) {
      debugLog('Calling socket.end()');
      this.socket.end();
      this.socket = null;
      debugLog('Socket ended and nulled');
    }
    this.connectionState = 'disconnected';

    // Reject all pending requests
    debugLog(`Rejecting ${this.pendingRequests.size} pending requests`);
    for (const [requestId, pending] of this.pendingRequests) {
      clearTimeout(pending.timeoutId);
      pending.reject(new Error('Client disconnected'));
      this.pendingRequests.delete(requestId);
    }

    // Reject all queued messages
    debugLog(`Rejecting ${this.messageQueue.length} queued messages`);
    for (const msg of this.messageQueue) {
      msg.reject(new Error('Client disconnected before message could be sent'));
    }
    this.messageQueue = [];

    debugLog(`<<< DISCONNECT complete`);
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
   * If not connected, queues the message for later delivery (when connection establishes).
   *
   * @param type - Message type (e.g., 'write-log')
   * @param payload - Worker-specific payload
   * @param userId - Optional userId for this request (overrides default)
   * @returns Promise resolving to worker response
   * @throws {Error} if queue is full, timeout, or worker returns error
   */
  async send(
    type: string,
    payload: TReq,
    userId?: string
  ): Promise<WorkerResponse<TRes>> {
    debugLog(`send() called - type: ${type}, connected: ${this.isConnected()}`);

    if (!this.isConnected()) {
      debugLog(`send() not connected - queueing message (state: ${this.connectionState})`);
      return this.queueMessage(type, payload, userId);
    }

    const request: WorkerRequest<TReq> = {
      id: randomUUID(),
      type,
      timestamp: new Date().toISOString(),
      payload,
      userId: userId ?? this.defaultUserId
    };

    debugLog(`Created request with id: ${request.id}`);

    return new Promise((resolve, reject) => {
      // Set up timeout
      const timeoutId = setTimeout(() => {
        debugLog(`Request ${request.id} timed out after ${this.timeout}ms`);
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
      debugLog(`Calling socket.write() with ${json.length} bytes`);
      this.socket!.write(json, (err) => {
        if (err) {
          debugLog(`socket.write() error: ${err.message}`);
          clearTimeout(timeoutId);
          this.pendingRequests.delete(request.id);
          reject(err);
        } else {
          debugLog(`socket.write() callback - success, data sent`);
        }
      });
    });
  }

  // ============================================================================
  // Socket Event Handlers
  // ============================================================================

  private setupSocketHandlers(): void {
    if (!this.socket) {
      debugLog('setupSocketHandlers: socket is null');
      return;
    }

    debugLog('Setting up socket handlers');

    // Handle incoming data
    this.socket.on('data', (data) => {
      debugLog(`Received data: ${data.length} bytes`);
      this.buffer += data.toString();

      // Process complete lines (newline-delimited JSON)
      const lines = this.buffer.split('\n');
      this.buffer = lines.pop() || ''; // Keep incomplete line in buffer

      for (const line of lines) {
        if (!line.trim()) continue;

        try {
          const message = JSON.parse(line);

          if (isWorkerResponse(message)) {
            this.handleResponse(message as WorkerResponse<TRes>);
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
      debugLog(`Socket 'error' event: ${err.message}`);
      console.error('WorkerClient: Socket error:', err);
      this.connectionState = 'error';
      this.attemptReconnect();
    });

    // Handle socket close
    this.socket.on('close', () => {
      debugLog(`Socket 'close' event fired - state was: ${this.connectionState}`);
      this.connectionState = 'disconnected';
      this.attemptReconnect();
    });

    debugLog('Socket handlers setup complete');
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

  // ============================================================================
  // Message Queue
  // ============================================================================

  /**
   * Queue a message for later delivery when connection establishes.
   * Returns a promise that resolves when the message is eventually sent.
   */
  private queueMessage(
    type: string,
    payload: TReq,
    userId?: string
  ): Promise<WorkerResponse<TRes>> {
    return new Promise((resolve, reject) => {
      if (this.messageQueue.length >= this.maxQueueSize) {
        debugLog(`Queue full (${this.messageQueue.length}/${this.maxQueueSize}), rejecting message`);
        reject(new Error(`Worker message queue full (${this.maxQueueSize} messages)`));
        return;
      }

      debugLog(`Queuing message - type: ${type}, queue size: ${this.messageQueue.length + 1}`);
      this.messageQueue.push({
        type,
        payload,
        userId,
        resolve,
        reject
      });
    });
  }

  /**
   * Flush all queued messages after connection establishes.
   * Sends messages in FIFO order.
   */
  private flushQueue(): void {
    if (this.messageQueue.length === 0) {
      return;
    }

    debugLog(`Flushing ${this.messageQueue.length} queued messages`);
    const queuedMessages = [...this.messageQueue];
    this.messageQueue = [];

    for (const msg of queuedMessages) {
      debugLog(`Sending queued message - type: ${msg.type}`);
      this.send(msg.type, msg.payload, msg.userId)
        .then(msg.resolve)
        .catch(msg.reject);
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
