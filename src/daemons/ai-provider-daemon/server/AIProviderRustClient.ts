/**
 * AIProviderRustClient - IPC bridge to Rust AIProviderModule
 *
 * Single-purpose client for ai/* commands to the Rust continuum-core process.
 * Uses the same IPC protocol as ORMRustClient but focused on AI operations.
 *
 * ARCHITECTURE:
 * - TypeScript AIProviderDaemon delegates to this client
 * - This client sends JSON requests to continuum-core socket
 * - Rust AIProviderModule handles all provider selection and API calls
 * - NO FALLBACKS: If Rust fails, we fail. Period.
 *
 * This enables:
 * - Unified AI provider management in Rust
 * - Tool calling support with native JSON format
 * - Better performance (Rust handles HTTP, retries, etc.)
 * - Single source of truth for API keys via Rust secrets.rs
 */

import net from 'net';
import { resolveCoreEndpoint, connectToCoreEndpoint } from '../../../workers/continuum-core/bindings/modules/base';
import type {
  TextGenerationRequest,
} from '../shared/AIProviderTypesV2';
import type { TextGenerationResponse, RoutingInfo } from '../../../shared/generated/ai';

// Endpoint resolution: honors CONTINUUM_CORE_URL env (tcp://host:port for
// containerized callers on Mac where Unix sockets don't traverse Docker
// Desktop's VM boundary). Falls back to SOCKETS.CONTINUUM_CORE Unix path.
const CORE_ENDPOINT = resolveCoreEndpoint();

/**
 * Rust response wrapper
 */
interface RustIPCResponse<T = unknown> {
  success: boolean;
  result?: T;
  error?: string;
  requestId?: number;
}

/**
 * Provider info from Rust
 */
interface RustProviderInfo {
  id: string;
  name: string;
  defaultModel: string;
  capabilities: {
    textGeneration: boolean;
    chat: boolean;
    toolUse: boolean;
    vision: boolean;
    streaming: boolean;
    embeddings: boolean;
    isLocal: boolean;
    maxContextWindow: number;
  };
}

/**
 * Health check result from Rust
 */
interface RustHealthResult {
  provider: string;
  name: string;
  status: string;
  apiAvailable: boolean;
  responseTimeMs: number;
  message?: string;
}

/**
 * AIProviderRustClient - Singleton IPC client for AI operations
 */
export class AIProviderRustClient {
  private static instance: AIProviderRustClient | null = null;
  private socket: net.Socket | null = null;
  private buffer: Buffer = Buffer.alloc(0);
  private pendingRequests: Map<number, (result: RustIPCResponse<unknown>) => void> = new Map();
  private nextRequestId = 1;
  private connected = false;
  private connecting = false;
  private wasConnected = false;
  private reconnectAttempts = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;

  private constructor() {}

  /**
   * Get singleton instance
   */
  static getInstance(): AIProviderRustClient {
    if (!AIProviderRustClient.instance) {
      AIProviderRustClient.instance = new AIProviderRustClient();
    }
    return AIProviderRustClient.instance;
  }

  /**
   * Ensure connected to continuum-core
   */
  private async ensureConnected(): Promise<void> {
    if (this.connected) return;
    if (this.connecting) {
      await new Promise<void>((resolve, reject) => {
        const check = setInterval(() => {
          if (this.connected) {
            clearInterval(check);
            resolve();
          } else if (!this.connecting) {
            clearInterval(check);
            reject(new Error('Connection failed'));
          }
        }, 10);
      });
      return;
    }

    this.connecting = true;

    return new Promise((resolve, reject) => {
      this.socket = connectToCoreEndpoint(CORE_ENDPOINT);

      this.socket.on('connect', () => {
        this.connected = true;
        this.wasConnected = true;
        this.connecting = false;
        this.reconnectAttempts = 0;
        resolve();
      });

      this.socket.on('data', (data: Buffer) => {
        this.onData(data);
      });

      this.socket.on('error', (err) => {
        this.connecting = false;
        if (!this.wasConnected) {
          reject(err);
        }
      });

      this.socket.on('close', () => {
        this.connected = false;
        this.connecting = false;
        this.socket = null;
        // CRITICAL: reject all in-flight requests so callers fail fast.
        // Without this, aiGenerate callers hung forever when native core
        // restarted — the root cause of the persona-subs-die-on-restart bug.
        const err = new Error('AIProvider IPC socket closed — continuum-core restarted');
        for (const callback of this.pendingRequests.values()) {
          callback({ success: false, error: err.message });
        }
        this.pendingRequests.clear();
        if (this.wasConnected) {
          this.scheduleReconnect();
        }
      });

      setTimeout(() => {
        if (!this.connected) {
          this.connecting = false;
          reject(new Error(`Connection timeout to ${CORE_ENDPOINT.description}`));
        }
      }, 5000);
    });
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return;
    const delay = Math.min(1000 * Math.pow(2, this.reconnectAttempts), 15000);
    console.log(`[AIProviderRustClient] Reconnecting to continuum-core in ${delay}ms (attempt ${this.reconnectAttempts + 1})`);
    this.reconnectTimer = setTimeout(async () => {
      this.reconnectTimer = null;
      try {
        await this.ensureConnected();
        console.log('[AIProviderRustClient] Reconnected to continuum-core');
      } catch {
        this.reconnectAttempts++;
        if (this.reconnectAttempts < 20) {
          this.scheduleReconnect();
        } else {
          console.error('[AIProviderRustClient] Gave up reconnecting after 20 attempts');
        }
      }
    }, delay);
  }

  /**
   * Process incoming binary data with length-prefixed framing
   */
  private onData(data: Buffer): void {
    this.buffer = Buffer.concat([this.buffer, data]);

    while (this.buffer.length >= 4) {
      const totalLength = this.buffer.readUInt32BE(0);
      const frameEnd = 4 + totalLength;

      if (this.buffer.length < frameEnd) break;

      const payload = this.buffer.subarray(4, frameEnd);
      this.buffer = this.buffer.subarray(frameEnd);

      const separatorIndex = payload.indexOf(0);
      const jsonBytes = separatorIndex !== -1
        ? payload.subarray(0, separatorIndex)
        : payload;

      try {
        const jsonStr = jsonBytes.toString('utf8');
        const response = JSON.parse(jsonStr) as RustIPCResponse;
        this.handleResponse(response);
      } catch (e) {
        console.error('[AIProviderRustClient] Failed to parse response:', e);
      }
    }
  }

  private handleResponse(response: RustIPCResponse): void {
    if (response.requestId !== undefined) {
      const callback = this.pendingRequests.get(response.requestId);
      if (callback) {
        callback(response);
        this.pendingRequests.delete(response.requestId);
      }
    }
  }

  /**
   * Send request to Rust and wait for response
   */
  private async request<T>(command: Record<string, unknown>): Promise<RustIPCResponse<T>> {
    await this.ensureConnected();

    if (!this.socket) {
      throw new Error('Not connected to continuum-core');
    }

    const requestId = this.nextRequestId++;
    const requestWithId = { ...command, requestId };
    const json = JSON.stringify(requestWithId) + '\n';

    return new Promise((resolve, reject) => {
      this.pendingRequests.set(requestId, (result) => {
        resolve(result as RustIPCResponse<T>);
      });

      this.socket!.write(json, (err) => {
        if (err) {
          this.pendingRequests.delete(requestId);
          reject(err);
        }
      });

      // Timeout after 300 seconds (5 minutes) to allow for Candle queue wait
      // Multiple local personas (Helper AI, Teacher AI, CodeReview AI, Local Assistant)
      // all serialize on the Candle model lock, so requests can wait 2-3 minutes
      setTimeout(() => {
        if (this.pendingRequests.has(requestId)) {
          this.pendingRequests.delete(requestId);
          reject(new Error('Request timeout'));
        }
      }, 300000);
    });
  }

  // ─── AI Operations ──────────────────────────────────────────────────────────

  /**
   * Generate text using Rust AI provider
   * Supports tool calling via native JSON format
   *
   * Types are unified via ts-rs: Rust TextGenerationResponse === TS TextGenerationResponse.
   * Wire fields pass through directly — no manual mapping needed.
   */
  async generateText(request: TextGenerationRequest): Promise<TextGenerationResponse> {
    // Send wire-compatible fields to Rust (TS-only fields like intelligenceLevel are stripped)
    const response = await this.request<TextGenerationResponse>({
      command: 'ai/generate',
      messages: request.messages,
      systemPrompt: request.systemPrompt,
      model: request.model,
      provider: request.provider,
      temperature: request.temperature,
      maxTokens: request.maxTokens,
      topP: request.topP,
      stopSequences: request.stopSequences,
      tools: request.tools,
      toolChoice: request.toolChoice,
      activeAdapters: request.activeAdapters,
      requestId: request.requestId,
      userId: request.userId,
      roomId: request.roomId,
      purpose: request.purpose,
    });

    if (!response.success || !response.result) {
      throw new Error(response.error || 'AI generation failed');
    }

    // Rust returns TextGenerationResponse directly — types match, no conversion needed
    const result = response.result;

    // Ensure routing.adaptersApplied is always an array (Rust may omit it)
    if (result.routing && !result.routing.adaptersApplied) {
      result.routing.adaptersApplied = [];
    }

    return result;
  }

  /**
   * List available providers
   */
  async listProviders(): Promise<RustProviderInfo[]> {
    const response = await this.request<{
      success: boolean;
      providers: RustProviderInfo[];
      available: string[];
      count: number;
    }>({
      command: 'ai/providers/list',
    });

    if (!response.success || !response.result) {
      throw new Error(response.error || 'Failed to list providers');
    }

    return response.result.providers;
  }

  /**
   * Check provider health
   */
  async healthCheck(): Promise<RustHealthResult[]> {
    const response = await this.request<{
      success: boolean;
      providers: RustHealthResult[];
    }>({
      command: 'ai/providers/health',
    });

    if (!response.success || !response.result) {
      throw new Error(response.error || 'Health check failed');
    }

    return response.result.providers;
  }

  /**
   * List available models
   */
  async listModels(): Promise<Array<{
    id: string;
    name: string;
    provider: string;
    capabilities: string[];
    contextWindow: number;
    supportsTools: boolean;
  }>> {
    const response = await this.request<{
      success: boolean;
      models: Array<{
        id: string;
        name: string;
        provider: string;
        capabilities: string[];
        contextWindow: number;
        supportsTools: boolean;
      }>;
      count: number;
    }>({
      command: 'ai/models/list',
    });

    if (!response.success || !response.result) {
      throw new Error(response.error || 'Failed to list models');
    }

    return response.result.models;
  }

  /**
   * Close connection
   */
  disconnect(): void {
    if (this.socket) {
      this.socket.end();
      this.socket = null;
      this.connected = false;
    }
    AIProviderRustClient.instance = null;
  }

  /**
   * Check if connected
   */
  isConnected(): boolean {
    return this.connected;
  }
}
