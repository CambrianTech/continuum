/**
 * LocalClaudeCodeProvider — Claude Code Agent SDK powered by local Candle inference.
 *
 * Uses the same Claude Agent SDK as ClaudeCodeProvider, but routes requests
 * to our local Anthropic-compatible HTTP endpoint (Candle + optional LoRA).
 *
 * Architecture:
 *   Sentinel Pipeline
 *     → CodingAgent step (provider: 'local-claude-code')
 *       → This provider starts/reuses Anthropic-compat HTTP endpoint
 *       → Sets ANTHROPIC_BASE_URL=http://localhost:{port}
 *       → Launches Claude Code via Agent SDK
 *         → Claude Code sends POST /v1/messages (Anthropic format)
 *           → Rust axum HTTP server
 *             → CandleAdapter.generate_text() with active LoRA adapter
 *               → Returns Anthropic-format response
 *
 * Training flywheel:
 *   captureTraining=true → persona/learning/capture-interaction
 *   → TrainingDataAccumulator → academy pipeline → improved LoRA → better coding
 */

import path from 'node:path';
import { spawn } from 'node:child_process';
import type {
  CodingAgentConfig,
  CodingAgentInteraction,
  CodingAgentProgressEvent,
  CodingAgentProvider,
  CodingAgentResult,
  CodingAgentToolCall,
} from './CodingAgentProvider';
import { Commands } from '@system/core/shared/Commands';
import { RustCoreIPCClient } from '../../../workers/continuum-core/bindings/RustCoreIPC';

export class LocalClaudeCodeProvider implements CodingAgentProvider {
  readonly providerId = 'local-claude-code';
  readonly providerName = 'Local Claude Code (Candle Inference)';

  async isAvailable(): Promise<boolean> {
    try {
      // Need the SDK installed
      await import('@anthropic-ai/claude-agent-sdk');
      // Server doesn't need to be running yet — we'll start it on demand
      return true;
    } catch {
      return false;
    }
  }

  async execute(
    config: CodingAgentConfig,
    onProgress?: (event: CodingAgentProgressEvent) => void,
  ): Promise<CodingAgentResult> {
    // Dynamic import — SDK is optional dependency
    const sdk = await import('@anthropic-ai/claude-agent-sdk');
    const { query } = sdk;
    type SDKOptions = import('@anthropic-ai/claude-agent-sdk').Options;
    type SDKSystemMessage = import('@anthropic-ai/claude-agent-sdk').SDKSystemMessage;
    type SDKAssistantMessage = import('@anthropic-ai/claude-agent-sdk').SDKAssistantMessage;
    type SDKUserMessage = import('@anthropic-ai/claude-agent-sdk').SDKUserMessage;
    type SDKResultMessage = import('@anthropic-ai/claude-agent-sdk').SDKResultMessage;
    type SDKResultSuccess = import('@anthropic-ai/claude-agent-sdk').SDKResultSuccess;
    type SpawnOptions = import('@anthropic-ai/claude-agent-sdk').SpawnOptions;
    type SpawnedProcess = import('@anthropic-ai/claude-agent-sdk').SpawnedProcess;
    type PermissionMode = import('@anthropic-ai/claude-agent-sdk').PermissionMode;

    const startTime = Date.now();
    const toolCalls: CodingAgentToolCall[] = [];
    const interactions: CodingAgentInteraction[] = [];
    let sessionId = '';
    let resultText = '';
    let totalCostUsd = 0;
    let numTurns = 0;
    let isError = false;
    let errorMessage: string | undefined;

    // ─── Start local inference HTTP server ────────────────────────────
    onProgress?.({
      type: 'status',
      message: 'Starting local inference server...',
      timestamp: Date.now(),
    });

    const ipc = await RustCoreIPCClient.getInstanceAsync();
    const startResult = await ipc.sentinelLocalInferenceStart();

    if (!startResult.success || !startResult.port) {
      return {
        success: false,
        text: '',
        sessionId: '',
        toolCalls: [],
        interactions: [],
        totalCostUsd: 0,
        numTurns: 0,
        durationMs: Date.now() - startTime,
        model: config.model || 'local',
        error: `Failed to start local inference: ${startResult.error || 'unknown'}`,
      };
    }

    const baseUrl = `http://127.0.0.1:${startResult.port}`;
    const localModel = config.model || 'local/default';

    onProgress?.({
      type: 'status',
      message: `Local inference ready on port ${startResult.port}`,
      timestamp: Date.now(),
    });

    // ─── Activate LoRA adapter if specified ───────────────────────────
    if (config.personaId && localModel.startsWith('local/')) {
      try {
        await ipc.request({
          command: 'ai/lora/activate',
          personaId: config.personaId,
          domain: 'coding',
        });
      } catch {
        // Non-fatal — may not have a trained adapter yet
        console.log(`[LocalClaudeCodeProvider] No LoRA adapter for persona ${config.personaId}, using base model`);
      }
    }

    // ─── Map permission mode ─────────────────────────────────────────
    const permissionModeMap: Record<string, PermissionMode> = {
      acceptEdits: 'acceptEdits',
      bypassPermissions: 'bypassPermissions',
      plan: 'plan',
      dontAsk: 'dontAsk',
    };
    const permissionMode: PermissionMode = permissionModeMap[config.permissionMode || ''] || 'default';

    // ─── Ensure PATH includes standard locations ─────────────────────
    const ensuredPath = ensurePath(process.env.PATH || '');
    process.env.PATH = ensuredPath;

    // ─── Build SDK options ───────────────────────────────────────────
    const options: Partial<SDKOptions> = {
      cwd: config.cwd,
      maxTurns: config.maxTurns,
      permissionMode,
      settingSources: ['project'],
      systemPrompt: config.systemPrompt
        ? { type: 'preset' as const, preset: 'claude_code' as const, append: config.systemPrompt }
        : { type: 'preset' as const, preset: 'claude_code' as const },
      model: localModel,
      env: {
        ...process.env,
        PATH: ensuredPath,
        CLAUDECODE: undefined,
        // Route to local inference endpoint
        ANTHROPIC_BASE_URL: baseUrl,
        // Dummy key — local server doesn't validate
        ANTHROPIC_API_KEY: 'sk-local-candle-inference',
      },
      stderr: (data: string) => {
        console.error(`[LocalClaudeCodeProvider] stderr: ${data.substring(0, 500)}`);
      },
      spawnClaudeCodeProcess: (spawnOpts: SpawnOptions): SpawnedProcess => {
        const command = spawnOpts.command === 'node'
          ? process.execPath
          : spawnOpts.command;
        console.log(`[LocalClaudeCodeProvider] Spawning: ${command} (cwd: ${spawnOpts.cwd}, baseUrl: ${baseUrl})`);
        const proc = spawn(command, spawnOpts.args, {
          cwd: spawnOpts.cwd,
          stdio: ['pipe', 'pipe', 'pipe'],
          env: spawnOpts.env as NodeJS.ProcessEnv,
          signal: spawnOpts.signal,
        });
        proc.stderr?.on('data', (chunk: Buffer) => {
          console.error(`[LocalClaudeCodeProvider] proc.stderr: ${chunk.toString().substring(0, 500)}`);
        });
        return {
          stdin: proc.stdin!,
          stdout: proc.stdout!,
          get killed() { return proc.killed; },
          get exitCode() { return proc.exitCode; },
          kill: (signal: NodeJS.Signals) => proc.kill(signal),
          on: proc.on.bind(proc) as SpawnedProcess['on'],
          once: proc.once.bind(proc) as SpawnedProcess['once'],
          off: proc.off.bind(proc) as SpawnedProcess['off'],
        };
      },
    };

    if (config.allowedTools && config.allowedTools.length > 0) {
      options.allowedTools = config.allowedTools;
    }

    if (config.resumeSessionId) {
      options.resume = config.resumeSessionId;
    }

    if (permissionMode === 'bypassPermissions') {
      options.allowDangerouslySkipPermissions = true;
    }

    // Don't set maxBudgetUsd — local inference is free
    // (setting it would cause Claude Code to try tracking costs against the local endpoint)

    // Record the user prompt as first interaction
    interactions.push({
      role: 'user',
      content: config.prompt,
      timestamp: Date.now(),
    });

    // ─── Execute via Agent SDK ───────────────────────────────────────
    const conversation = query({
      prompt: config.prompt,
      options: options as SDKOptions,
    });

    try {
      for await (const message of conversation) {
        switch (message.type) {
          case 'system': {
            if (message.subtype === 'init') {
              const initMsg = message as SDKSystemMessage;
              sessionId = initMsg.session_id;
              onProgress?.({
                type: 'status',
                message: `Local session initialized: ${sessionId}`,
                timestamp: Date.now(),
              });
            }
            break;
          }

          case 'assistant': {
            const assistantMsg = message as SDKAssistantMessage;
            sessionId = assistantMsg.session_id || sessionId;

            const content = assistantMsg.message?.content;
            if (Array.isArray(content)) {
              const textParts: string[] = [];

              for (const block of content) {
                if (block.type === 'text') {
                  textParts.push(block.text);
                } else if (block.type === 'tool_use') {
                  onProgress?.({
                    type: 'tool_start',
                    toolName: block.name,
                    message: `Calling ${block.name}`,
                    timestamp: Date.now(),
                  });

                  toolCalls.push({
                    toolName: block.name,
                    input: block.input as Record<string, unknown>,
                    output: '',
                    isError: false,
                    durationMs: 0,
                  });
                }
              }

              if (textParts.length > 0) {
                const text = textParts.join('\n');
                interactions.push({
                  role: 'assistant',
                  content: text,
                  timestamp: Date.now(),
                });
                onProgress?.({
                  type: 'assistant_message',
                  message: text.substring(0, 200),
                  timestamp: Date.now(),
                });
              }
            }
            break;
          }

          case 'user': {
            const userMsg = message as SDKUserMessage;
            if (userMsg.tool_use_result !== undefined && toolCalls.length > 0) {
              const lastTool = toolCalls[toolCalls.length - 1];
              const resultStr = typeof userMsg.tool_use_result === 'string'
                ? userMsg.tool_use_result
                : JSON.stringify(userMsg.tool_use_result);
              lastTool.output = resultStr;
              lastTool.durationMs = Date.now() - (interactions[interactions.length - 1]?.timestamp || Date.now());

              onProgress?.({
                type: 'tool_end',
                toolName: lastTool.toolName,
                message: `${lastTool.toolName} completed`,
                timestamp: Date.now(),
              });
            }
            break;
          }

          case 'result': {
            const result = message as SDKResultMessage;
            sessionId = result.session_id || sessionId;
            numTurns = result.num_turns || 0;
            totalCostUsd = 0; // Local inference is free
            isError = result.is_error || false;

            if (result.subtype === 'success') {
              resultText = result.result || '';
            } else {
              isError = true;
              const errorResult = result as Exclude<SDKResultMessage, SDKResultSuccess>;
              errorMessage = errorResult.errors?.join('; ') || `Agent ended with: ${result.subtype}`;
              resultText = errorMessage;
            }
            break;
          }
        }
      }
    } catch (iterError: unknown) {
      const msg = iterError instanceof Error ? iterError.message : String(iterError);
      if (resultText && !isError) {
        console.log(`[LocalClaudeCodeProvider] Process exit error after successful result (ignoring): ${msg}`);
      } else if (!resultText && !isError) {
        isError = true;
        errorMessage = msg;
        resultText = msg;
      }
    }

    const durationMs = Date.now() - startTime;

    // Attach tool calls to the last assistant interaction
    if (toolCalls.length > 0 && interactions.length > 0) {
      const lastAssistant = [...interactions].reverse().find(i => i.role === 'assistant');
      if (lastAssistant) {
        lastAssistant.toolCalls = toolCalls;
      }
    }

    return {
      success: !isError,
      text: resultText,
      sessionId,
      toolCalls,
      interactions,
      totalCostUsd: 0, // Local inference is free
      numTurns,
      durationMs,
      model: localModel,
      error: errorMessage,
    };
  }
}

/**
 * Ensure PATH includes standard binary locations for daemon contexts.
 */
function ensurePath(currentPath: string): string {
  const nodeDir = path.dirname(process.execPath);
  const requiredDirs = [
    nodeDir,
    '/opt/homebrew/bin',
    '/usr/local/bin',
    '/usr/bin',
    `${process.env.HOME}/.local/bin`,
    `${process.env.HOME}/.nvm/current/bin`,
  ];
  const pathDirs = new Set(currentPath.split(':'));
  for (const dir of requiredDirs) {
    if (dir && !pathDirs.has(dir)) {
      pathDirs.add(dir);
    }
  }
  return Array.from(pathDirs).join(':');
}
