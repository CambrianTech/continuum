/**
 * PromptCapture — Records every LLM prompt for inspection and replay
 *
 * Every prompt sent to any model is captured as a structured JSONL entry.
 * This enables:
 * - Debugging: inspect exactly what any persona saw before responding
 * - Replay: re-run any prompt against the same or different model
 * - Scenario testing: replay entire conversation sequences
 * - Regression: compare outputs before/after RAG changes
 *
 * Captures are written to `.continuum/jtag/logs/system/prompt-captures.jsonl`
 * One JSON object per line — standard JSONL format for easy streaming/parsing.
 *
 * Usage:
 *   PromptCapture.capture({ personaId, personaName, model, ... });
 *
 * Replay:
 *   const captures = await PromptCapture.load({ personaName: 'Helper AI', limit: 5 });
 *   for (const capture of captures) {
 *     const response = await AIProviderDaemon.generateText(capture.request);
 *   }
 */

import * as fs from 'fs';
import * as path from 'path';
import * as readline from 'readline';
import { Logger } from '../../core/logging/Logger';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { SystemPaths } from '../../core/config/SystemPaths';

const log = Logger.create('PromptCapture', 'rag');

/** Maximum capture file size before rotation (50MB — not 7GB) */
const MAX_FILE_SIZE_BYTES = 50 * 1024 * 1024;

/** Maximum entries queued in memory before forced flush */
const MAX_WRITE_QUEUE = 20;

/** Rotated files kept (prompt-captures.1.jsonl, .2.jsonl, etc.) */
const MAX_ROTATED_FILES = 3;

/**
 * A captured LLM prompt — contains everything needed to replay the request.
 */
export interface CapturedPrompt {
  /** Unique capture ID (ISO timestamp + short persona ID for dedup) */
  id: string;
  /** When the prompt was sent */
  timestamp: string;
  /** Persona that generated this prompt */
  personaId: UUID;
  personaName: string;
  /** Model and provider configuration */
  model: string;
  provider: string;
  temperature: number;
  maxTokens: number;
  /** The complete system prompt */
  systemPrompt: string;
  /** Conversation messages (role + content + name) */
  messages: Array<{
    role: 'system' | 'user' | 'assistant';
    content: string;
    name?: string;
  }>;
  /** Tool definitions (native JSON specs or XML in system prompt) */
  tools?: unknown[];
  toolChoice?: string;
  /** What triggered this generation */
  triggerMessageId?: UUID;
  triggerMessagePreview?: string;
  /** RAG metadata for context */
  ragSourceCount?: number;
  ragTotalTokens?: number;
  /** Active LoRA adapters (if any) */
  activeAdapters?: Array<{ name: string; path: string }>;
}

/**
 * Filter options for loading captures.
 */
export interface CaptureFilter {
  personaName?: string;
  personaId?: UUID;
  model?: string;
  provider?: string;
  /** Only captures after this timestamp */
  after?: Date;
  /** Only captures before this timestamp */
  before?: Date;
  /** Max captures to return (newest first) */
  limit?: number;
}

export class PromptCapture {
  private static _captureFile: string | null = null;
  private static _writeQueue: string[] = [];
  private static _flushTimer: ReturnType<typeof setTimeout> | null = null;
  /** Whether capture is enabled. Defaults to false — opt-in only. */
  private static _enabled = false;

  /** Enable or disable prompt capture at runtime */
  static set enabled(value: boolean) {
    this._enabled = value;
    if (value) {
      log.info('Prompt capture enabled');
    } else {
      // Flush anything pending before disabling
      this.flush();
      log.info('Prompt capture disabled');
    }
  }

  static get enabled(): boolean {
    return this._enabled;
  }

  /** Get the capture file path, creating the directory if needed */
  private static captureFile(): string {
    if (!this._captureFile) {
      const logsDir = SystemPaths.logs.system;
      const dir = path.dirname(logsDir);
      if (!fs.existsSync(dir)) {
        fs.mkdirSync(dir, { recursive: true });
      }
      this._captureFile = path.join(dir, 'prompt-captures.jsonl');
    }
    return this._captureFile;
  }

  /**
   * Rotate the capture file if it exceeds MAX_FILE_SIZE_BYTES.
   * Keeps up to MAX_ROTATED_FILES old files.
   */
  private static rotateIfNeeded(): void {
    const filePath = this.captureFile();
    try {
      if (!fs.existsSync(filePath)) return;
      const stat = fs.statSync(filePath);
      if (stat.size < MAX_FILE_SIZE_BYTES) return;

      const dir = path.dirname(filePath);
      const base = path.basename(filePath, '.jsonl');

      // Shift existing rotated files (delete oldest if at limit)
      for (let i = MAX_ROTATED_FILES; i >= 1; i--) {
        const older = path.join(dir, `${base}.${i}.jsonl`);
        if (i === MAX_ROTATED_FILES) {
          if (fs.existsSync(older)) fs.unlinkSync(older);
        } else {
          const newer = path.join(dir, `${base}.${i + 1}.jsonl`);
          if (fs.existsSync(older)) fs.renameSync(older, newer);
        }
      }

      // Current → .1
      fs.renameSync(filePath, path.join(dir, `${base}.1.jsonl`));
      log.info(`Rotated prompt capture file (was ${(stat.size / 1024 / 1024).toFixed(1)}MB)`);
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      log.warn(`Failed to rotate capture file: ${msg}`);
    }
  }

  /**
   * Capture a prompt — fire-and-forget, non-blocking.
   * Extracts system prompt from messages array, serializes to JSONL.
   *
   * No-op when capture is disabled (default). Enable with:
   *   PromptCapture.enabled = true;
   */
  static capture(params: {
    personaId: UUID;
    personaName: string;
    model: string;
    provider: string;
    temperature: number;
    maxTokens: number;
    messages: Array<{ role: string; content: unknown; name?: string }>;
    tools?: unknown[];
    toolChoice?: string;
    triggerMessageId?: UUID;
    triggerMessagePreview?: string;
    ragSourceCount?: number;
    ragTotalTokens?: number;
    activeAdapters?: Array<{ name: string; path: string }>;
  }): void {
    if (!this._enabled) return;

    try {
      const now = new Date();
      const shortId = params.personaId.slice(0, 8);

      // Extract system prompt from first system message
      let systemPrompt = '';
      const conversationMessages: CapturedPrompt['messages'] = [];

      for (const msg of params.messages) {
        const content = typeof msg.content === 'string'
          ? msg.content
          : JSON.stringify(msg.content);

        if (msg.role === 'system' && !systemPrompt) {
          systemPrompt = content;
        } else {
          conversationMessages.push({
            role: msg.role as 'system' | 'user' | 'assistant',
            content,
            name: msg.name
          });
        }
      }

      const capture: CapturedPrompt = {
        id: `${now.toISOString()}_${shortId}`,
        timestamp: now.toISOString(),
        personaId: params.personaId,
        personaName: params.personaName,
        model: params.model,
        provider: params.provider,
        temperature: params.temperature,
        maxTokens: params.maxTokens,
        systemPrompt,
        messages: conversationMessages,
        tools: params.tools,
        toolChoice: params.toolChoice,
        triggerMessageId: params.triggerMessageId,
        triggerMessagePreview: params.triggerMessagePreview,
        ragSourceCount: params.ragSourceCount,
        ragTotalTokens: params.ragTotalTokens,
        activeAdapters: params.activeAdapters
      };

      const line = JSON.stringify(capture);
      this._writeQueue.push(line);

      // Force flush if queue is getting large (bounded memory)
      if (this._writeQueue.length >= MAX_WRITE_QUEUE) {
        this.flush();
        return;
      }

      // Flush every 500ms (batches multiple captures from concurrent personas)
      if (!this._flushTimer) {
        this._flushTimer = setTimeout(() => this.flush(), 500);
      }
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      log.warn(`Failed to capture prompt: ${msg}`);
    }
  }

  /** Flush queued captures to disk */
  private static flush(): void {
    if (this._flushTimer) {
      clearTimeout(this._flushTimer);
      this._flushTimer = null;
    }
    if (this._writeQueue.length === 0) return;

    const lines = this._writeQueue.splice(0);
    const data = lines.join('\n') + '\n';

    try {
      this.rotateIfNeeded();
      fs.appendFileSync(this.captureFile(), data, 'utf-8');
    } catch (error: unknown) {
      const msg = error instanceof Error ? error.message : String(error);
      log.warn(`Failed to write prompt captures: ${msg}`);
    }
  }

  /**
   * Load captured prompts matching filter criteria.
   * Streams the JSONL file line-by-line to avoid loading the entire file into memory.
   * Returns newest first.
   */
  static async load(filter?: CaptureFilter): Promise<CapturedPrompt[]> {
    // Flush any pending writes first
    this.flush();

    const filePath = this.captureFile();
    if (!fs.existsSync(filePath)) return [];

    const captures: CapturedPrompt[] = [];
    const limit = filter?.limit && filter.limit > 0 ? filter.limit : Infinity;

    const afterMs = filter?.after ? filter.after.getTime() : -Infinity;
    const beforeMs = filter?.before ? filter.before.getTime() : Infinity;

    const rl = readline.createInterface({
      input: fs.createReadStream(filePath, { encoding: 'utf-8' }),
      crlfDelay: Infinity,
    });

    for await (const line of rl) {
      if (line.length === 0) continue;

      let capture: CapturedPrompt;
      try {
        capture = JSON.parse(line);
      } catch {
        continue; // Skip malformed lines
      }

      // Apply filters inline (avoid accumulating everything then filtering)
      if (filter?.personaName && capture.personaName !== filter.personaName) continue;
      if (filter?.personaId && capture.personaId !== filter.personaId) continue;
      if (filter?.model && capture.model !== filter.model) continue;
      if (filter?.provider && capture.provider !== filter.provider) continue;

      const ts = new Date(capture.timestamp).getTime();
      if (ts < afterMs || ts > beforeMs) continue;

      captures.push(capture);
    }

    // Newest first
    captures.reverse();

    // Apply limit after reverse (we want newest N)
    if (captures.length > limit) {
      captures.length = limit;
    }

    return captures;
  }

  /**
   * Reconstruct a full TextGenerationRequest from a captured prompt.
   * This is what you pass to AIProviderDaemon.generateText() for replay.
   */
  static toReplayRequest(capture: CapturedPrompt): {
    messages: Array<{ role: string; content: string }>;
    model: string;
    temperature: number;
    maxTokens: number;
    provider: string;
    tools?: unknown[];
    toolChoice?: string;
  } {
    // Rebuild the messages array with system prompt first
    const messages: Array<{ role: string; content: string }> = [
      { role: 'system', content: capture.systemPrompt }
    ];

    for (const msg of capture.messages) {
      messages.push({
        role: msg.role,
        content: msg.content
      });
    }

    return {
      messages,
      model: capture.model,
      temperature: capture.temperature,
      maxTokens: capture.maxTokens,
      provider: capture.provider,
      tools: capture.tools,
      toolChoice: capture.toolChoice
    };
  }

  /**
   * Get a human-readable summary of a capture (for CLI/logging).
   */
  static summarize(capture: CapturedPrompt): string {
    const promptChars = capture.systemPrompt.length;
    const msgCount = capture.messages.length;
    const toolCount = capture.tools?.length ?? 0;
    const trigger = capture.triggerMessagePreview
      ? `"${capture.triggerMessagePreview.slice(0, 60)}..."`
      : 'unknown';

    return [
      `[${capture.timestamp}] ${capture.personaName} → ${capture.model} (${capture.provider})`,
      `  System prompt: ${promptChars} chars (~${Math.ceil(promptChars / 4)} tokens)`,
      `  Messages: ${msgCount}, Tools: ${toolCount}, MaxTokens: ${capture.maxTokens}`,
      `  Trigger: ${trigger}`,
      capture.activeAdapters?.length
        ? `  LoRA: ${capture.activeAdapters.map(a => a.name).join(', ')}`
        : null
    ].filter(Boolean).join('\n');
  }
}
