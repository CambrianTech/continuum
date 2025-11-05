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
 * Phase 3: Add real Ollama inference
 */

import { Worker } from 'worker_threads';
import { EventEmitter } from 'events';
import * as path from 'path';

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
}

interface ProviderConfig {
  baseUrl?: string;
  maxConcurrent?: number;
  model?: string;
}

interface WorkerConfig {
  providerType?: 'ollama' | 'openai' | 'anthropic' | 'mock';
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
 *     providerType: 'ollama',
 *     providerConfig: { baseUrl: 'http://localhost:11434', maxConcurrent: 1 }
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
    // Resolve worker script path relative to this file
    const workerPath = path.join(__dirname, 'persona-worker.js');

    console.log(`🧵 Starting worker for persona ${this.personaId}`);
    console.log(`   Worker script: ${workerPath}`);
    console.log(`   Provider type: ${this.config.providerType}`);

    this.worker = new Worker(workerPath, {
      workerData: {
        personaId: this.personaId,
        providerType: this.config.providerType,
        providerConfig: this.config.providerConfig
      }
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

    this.worker.postMessage({
      type: 'evaluate',
      message: message,
      timestamp: startTime
    });

    // Wait for result (with timeout)
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error(`Worker ${this.personaId} did not respond to evaluate within ${timeoutMs}ms`));
      }, timeoutMs);

      const handler = (msg: WorkerResponse) => {
        if (msg.type === 'result' && msg.data) {
          const data = msg.data as any;
          if (data.messageId === message.id) {
            clearTimeout(timeout);
            this.removeListener('message', handler);

            const totalLatency = Date.now() - startTime;
            console.log(`📊 Worker ${this.personaId}: Evaluation complete in ${totalLatency}ms`);

            resolve(data);
          }
        }
      };

      this.on('message', handler);
    });
  }
}
