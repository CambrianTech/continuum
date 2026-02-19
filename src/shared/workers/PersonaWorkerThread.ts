/**
 * PersonaWorkerThread
 * ===================
 *
 * Manages a single PersonaUser worker thread.
 * Handles bidirectional communication with worker.
 *
 * Similar to CBAR's QueueThread<T> pattern.
 *
 * Phase 1: Skeleton implementation (ping-pong only)
 * Phase 2: Add message evaluation
 * Phase 3: Add real Candle inference
 */

import { Worker } from 'worker_threads';
import { EventEmitter } from 'events';
import * as path from 'path';
import { fileURLToPath } from 'url';
import { getResourceManager } from '../../system/resources/shared/ResourceManager';
import type { ResourceDecision } from '../../system/resources/shared/ResourceModerator';

interface WorkerMessage {
  type: 'ping' | 'evaluate' | 'shutdown';
  timestamp: number;
  data?: unknown;
}

interface WorkerResponse {
  type: 'ready' | 'pong' | 'result' | 'error';
  timestamp: number;
  personaId?: string;
  receivedAt?: number;
  latency?: number;
  data?: unknown;
  error?: string;
}

interface ProviderConfig {
  apiEndpoint?: string; // Changed from baseUrl to match worker implementation
  model?: string;
}

interface WorkerConfig {
  providerType?: 'candle' | 'local' | 'openai' | 'anthropic' | 'mock';
  providerConfig?: ProviderConfig;
}

/**
 * Manages a single PersonaUser worker thread.
 *
 * Usage:
 *   const worker = new PersonaWorkerThread('persona-id-123');
 *   await worker.start();  // Wait for ready
 *   const latency = await worker.ping();  // Test communication
 *   await worker.shutdown();  // Clean termination
 *
 * Phase 3 Usage (with provider config):
 *   const worker = new PersonaWorkerThread('persona-id-123', {
 *     providerType: 'candle',
 *     providerConfig: { model: 'llama3.2:1b' }
 *   });
 */
export class PersonaWorkerThread extends EventEmitter {
  private worker: Worker | null = null;
  private personaId: string;
  private isReady: boolean = false;
  private messageCount: number = 0;
  private config: WorkerConfig;

  constructor(personaId: string, config: WorkerConfig = {}) {
    super();
    this.personaId = personaId;
    this.config = {
      providerType: config.providerType || 'mock',
      providerConfig: config.providerConfig || {}
    };
  }

  /**
   * Start the worker and wait for ready signal.
   * Times out after 5 seconds if worker doesn't signal ready.
   */
  async start(): Promise<void> {
    // Load JS worker (pragmatic: one small JS file, imports from compiled TS)
    const currentDir = path.dirname(fileURLToPath(import.meta.url));
    const workerPath = path.join(currentDir, 'persona-worker.js');

    console.log(`🧵 Starting worker for persona ${this.personaId.slice(0, 8)} (${this.config.providerType})`);

    this.worker = new Worker(workerPath, {
      workerData: {
        personaId: this.personaId,
        providerType: this.config.providerType,
        providerConfig: this.config.providerConfig
      }
      // No execArgv needed - worker is compiled JS importing compiled JS
    });

    // Listen for messages from worker
    this.worker.on('message', (msg: WorkerResponse) => {
      this.handleWorkerMessage(msg);
    });

    this.worker.on('error', (error) => {
      console.error(`❌ Worker error for ${this.personaId}:`, error);
      this.emit('error', error);
    });

    this.worker.on('exit', (code) => {
      console.log(`🧵 Worker ${this.personaId} exited with code ${code}`);
      this.emit('exit', code);
    });

    // Wait for ready signal (with timeout)
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Worker ${this.personaId} did not signal ready within 5s`));
      }, 5000);

      this.once('ready', () => {
        clearTimeout(timeout);
        resolve();
      });
    });
  }

  /**
   * Handle messages received from worker thread.
   */
  private handleWorkerMessage(msg: WorkerResponse): void {
    console.log(`📨 Main thread received from worker ${this.personaId}: type=${msg.type}`);

    if (msg.type === 'ready') {
      this.isReady = true;
      console.log(`✅ Worker ${this.personaId} is ready`);
      this.emit('ready');
    }
    else if (msg.type === 'pong') {
      const latency = Date.now() - (msg.receivedAt || msg.timestamp);
      console.log(`🏓 Pong from ${this.personaId}: round-trip=${latency}ms`);
      this.emit('pong', msg);
    }
    else if (msg.type === 'result') {
      // Evaluation result from worker
      console.log(`📊 Result from ${this.personaId}: ${JSON.stringify(msg.data).substring(0, 100)}...`);
      this.emit('message', msg);
    }
    else {
      // Forward other message types to listeners
      this.emit('message', msg);
    }
  }

  /**
   * Send ping to worker and measure round-trip latency.
   * Returns latency in milliseconds.
   */
  async ping(): Promise<number> {
    if (!this.isReady || !this.worker) {
      throw new Error(`Worker ${this.personaId} not ready`);
    }

    const startTime = Date.now();
    this.messageCount++;

    this.worker.postMessage({
      type: 'ping',
      timestamp: startTime
    });

    // Wait for pong response (with timeout)
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Worker ${this.personaId} did not respond to ping within 1s`));
      }, 1000);

      const handler = (msg: WorkerResponse) => {
        if (msg.type === 'pong') {
          clearTimeout(timeout);
          this.removeListener('pong', handler);

          const latency = Date.now() - startTime;
          resolve(latency);
        }
      };

      this.on('pong', handler);
    });
  }

  /**
   * Terminate the worker thread cleanly.
   */
  async shutdown(): Promise<void> {
    if (!this.worker) {
      return;
    }

    console.log(`🛑 Shutting down worker ${this.personaId}`);

    // Send shutdown message (optional - worker will terminate anyway)
    try {
      this.worker.postMessage({ type: 'shutdown', timestamp: Date.now() });
    } catch (error) {
      // Worker may have already exited
    }

    // Terminate worker
    await this.worker.terminate();
    this.worker = null;
    this.isReady = false;

    console.log(`✅ Worker ${this.personaId} shut down`);
  }

  /**
   * Check if worker is ready to receive messages.
   */
  isWorkerReady(): boolean {
    return this.isReady && this.worker !== null;
  }

  /**
   * Get number of messages sent to this worker.
   */
  getMessageCount(): number {
    return this.messageCount;
  }

  /**
   * Evaluate a message and get persona's decision.
   * Returns evaluation result with confidence and reasoning.
   *
   * @param message Message to evaluate
   * @param timeoutMs Optional timeout in milliseconds (default: 5000)
   */
  async evaluateMessage(message: any, timeoutMs: number = 5000): Promise<any> {
    if (!this.isReady || !this.worker) {
      throw new Error(`Worker ${this.personaId} not ready`);
    }

    const startTime = Date.now();
    this.messageCount++;

    // Send evaluation request to worker with context
    // Worker builds its own prompt for real inference, or uses smart heuristics
    this.worker.postMessage({
      type: 'evaluate',
      message: {
        id: message.id,
        content: message.content,
        senderId: message.senderId,
        timestamp: message.timestamp
      },
      // Pass PersonaState for smarter heuristics
      personaState: message.personaState || {
        energy: 0.8,
        attention: 0.7,
        mood: 'active'
      },
      // Pass room/config settings
      config: message.config || {
        responseThreshold: 50,
        temperature: 0.7
      },
      timestamp: startTime
    });

    // Wait for result and parse it (parsing logic - not in worker)
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Worker ${this.personaId} did not respond within ${timeoutMs}ms`));
      }, timeoutMs);

      const handler = (msg: WorkerResponse) => {
        if (msg.type === 'result') {
          const data = msg.data as any;

          clearTimeout(timeout);
          this.removeListener('message', handler);

          const totalLatency = Date.now() - startTime;
          console.log(`📊 Worker ${this.personaId}: Evaluation complete in ${totalLatency}ms`);

          // Worker returns structured data - just pass it through
          resolve({
            messageId: data.messageId || message.id,
            confidence: data.confidence,
            shouldRespond: data.shouldRespond,
            reasoning: data.reasoning,
            processingTime: data.processingTime || totalLatency
          });
        }
        else if (msg.type === 'error') {
          clearTimeout(timeout);
          this.removeListener('message', handler);
          reject(new Error(`Worker error: ${msg.error || 'Unknown error'}`));
        }
      };

      this.on('message', handler);
    });
  }

  /**
   * Check if worker is available to accept new evaluation requests
   *
   * Uses ResourceManager to check:
   * - Worker thread availability
   * - GPU memory quota
   * - Throttle status (failure rate)
   *
   * This is the mechanical boundary - adapters decide if they can evaluate
   */
  isAvailable(): boolean {
    // Basic check: worker must be ready
    if (!this.isReady || !this.worker) {
      return false;
    }

    // Resource check: delegate to ResourceManager + ResourceModerator
    try {
      const resourceManager = getResourceManager();
      return resourceManager.isAvailable(this.personaId);
    } catch (error) {
      // Graceful fallback: If ResourceManager not available, just check worker ready state
      // This happens during early initialization before PersonaUser.initialize() runs
      console.warn(`⚠️  Worker ${this.personaId.slice(0, 8)}: ResourceManager not available, using simple check`);
      return true; // Default to available if resource system not initialized
    }
  }
}
