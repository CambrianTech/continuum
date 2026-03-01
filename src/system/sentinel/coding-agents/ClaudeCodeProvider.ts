/**
 * ClaudeCodeProvider — Claude Agent SDK wrapper implementing CodingAgentProvider.
 *
 * Spawns Claude Code as a child process via the SDK. All heavy work happens
 * in that child process — our event loop just awaits async results.
 *
 * Dynamic import so the SDK is an optional dependency: if not installed,
 * isAvailable() returns false and the system degrades gracefully.
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

export class ClaudeCodeProvider implements CodingAgentProvider {
  readonly providerId = 'claude-code';
  readonly providerName = 'Claude Code (Agent SDK)';

  async isAvailable(): Promise<boolean> {
    try {
      await import('@anthropic-ai/claude-agent-sdk');
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

    const startTime = Date.now();
    const toolCalls: CodingAgentToolCall[] = [];
    const interactions: CodingAgentInteraction[] = [];
    let sessionId = '';
    let resultText = '';
    let totalCostUsd = 0;
    let numTurns = 0;
    let model = config.model || 'sonnet';
    let isError = false;
    let errorMessage: string | undefined;

    // Map our permission mode to SDK PermissionMode
    const permissionMode = this.mapPermissionMode(config.permissionMode);

    // Ensure PATH includes standard locations — nohup/daemon contexts can strip PATH.
    // CRITICAL: Must set process.env.PATH directly because the SDK uses the PARENT
    // process's PATH to locate the node binary BEFORE spawning the child process.
    // The env option only controls the child's environment, not the SDK's lookup.
    const ensuredPath = this.ensurePath(process.env.PATH || '');
    process.env.PATH = ensuredPath;

    // Build SDK options
    const options: Record<string, unknown> = {
      cwd: config.cwd,
      maxTurns: config.maxTurns,
      maxBudgetUsd: config.maxBudgetUsd,
      permissionMode,
      settingSources: ['project'],
      systemPrompt: config.systemPrompt
        ? { type: 'preset' as const, preset: 'claude_code' as const, append: config.systemPrompt }
        : { type: 'preset' as const, preset: 'claude_code' as const },
      env: {
        ...process.env,
        PATH: ensuredPath,
        // Strip CLAUDECODE to prevent "nested session" detection.
        // The daemon inherits this env var when launched from a Claude Code session.
        CLAUDECODE: undefined,
        // Strip ANTHROPIC_API_KEY so Claude Code uses its own auth (Max subscription = unlimited).
        // API key is for base model inference (ai/generate); coding agents use OAuth from `claude login`.
        // Users who want API key billing can set apiKey on the CodingAgentConfig explicitly.
        ANTHROPIC_API_KEY: config.apiKey || undefined,
      },
      // Capture stderr for diagnostics
      stderr: (data: string) => {
        console.error(`[ClaudeCodeProvider] stderr: ${data.substring(0, 500)}`);
      },
      // Custom spawn: resolve 'node' to absolute path via process.execPath.
      // In daemon contexts (nohup), PATH is minimal and spawn('node', ...) fails
      // with ENOENT. Using process.execPath gives the absolute path to the running
      // node binary, completely bypassing PATH resolution.
      spawnClaudeCodeProcess: (spawnOpts: {
        command: string; args: string[]; cwd?: string;
        env: Record<string, string | undefined>; signal: AbortSignal;
      }) => {
        const command = spawnOpts.command === 'node'
          ? process.execPath
          : spawnOpts.command;
        const hasApiKey = !!spawnOpts.env.ANTHROPIC_API_KEY;
        console.log(`[ClaudeCodeProvider] Spawning: ${command} ${spawnOpts.args.slice(0, 3).join(' ')}... (cwd: ${spawnOpts.cwd}, hasApiKey: ${hasApiKey})`);
        const proc = spawn(command, spawnOpts.args, {
          cwd: spawnOpts.cwd,
          stdio: ['pipe', 'pipe', 'pipe'],
          env: spawnOpts.env as NodeJS.ProcessEnv,
          signal: spawnOpts.signal,
        });
        // Capture stderr for debugging
        proc.stderr?.on('data', (chunk: Buffer) => {
          console.error(`[ClaudeCodeProvider] proc.stderr: ${chunk.toString().substring(0, 500)}`);
        });
        return {
          stdin: proc.stdin,
          stdout: proc.stdout,
          get killed() { return proc.killed; },
          get exitCode() { return proc.exitCode; },
          kill: (signal: NodeJS.Signals) => proc.kill(signal),
          on: proc.on.bind(proc) as any,
          once: proc.once.bind(proc) as any,
          off: proc.off.bind(proc) as any,
        };
      },
    };

    if (config.model) {
      options.model = config.model;
    }

    if (config.allowedTools && config.allowedTools.length > 0) {
      options.allowedTools = config.allowedTools;
    }

    if (config.resumeSessionId) {
      options.resume = config.resumeSessionId;
    }

    if (permissionMode === 'bypassPermissions') {
      options.allowDangerouslySkipPermissions = true;
    }

    // Record the user prompt as first interaction
    interactions.push({
      role: 'user',
      content: config.prompt,
      timestamp: Date.now(),
    });

    // Stream SDK messages.
    // The SDK's async iterator yields all messages, then calls waitForExit() which
    // throws if the process exit code is non-zero. Claude Code can exit with code 1
    // even when the conversation succeeded (e.g., after a result:success message).
    // We catch this and check if we already have a valid result.
    const conversation = query({
      prompt: config.prompt,
      options: options as any,
    });

    try {
    for await (const message of conversation) {
      switch (message.type) {
        case 'system': {
          if (message.subtype === 'init') {
            sessionId = message.session_id;
            model = (message as any).model || model;
            onProgress?.({
              type: 'status',
              message: `Session initialized: ${sessionId}`,
              timestamp: Date.now(),
            });
          }
          break;
        }

        case 'assistant': {
          const assistantMsg = message as any;
          sessionId = assistantMsg.session_id || sessionId;

          // Extract text content and tool use from the message
          const content = assistantMsg.message?.content;
          if (Array.isArray(content)) {
            let textParts: string[] = [];

            for (const block of content) {
              if (block.type === 'text') {
                textParts.push(block.text);
              } else if (block.type === 'tool_use') {
                const toolStart = Date.now();
                onProgress?.({
                  type: 'tool_start',
                  toolName: block.name,
                  message: `Calling ${block.name}`,
                  timestamp: toolStart,
                });

                // Tool call will be completed when we see the user message with tool_use_result
                toolCalls.push({
                  toolName: block.name,
                  input: block.input as Record<string, unknown>,
                  output: '', // Filled when tool result arrives
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
          // Tool results come back as user messages
          const userMsg = message as any;
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
          const result = message as any;
          sessionId = result.session_id || sessionId;
          numTurns = result.num_turns || 0;
          totalCostUsd = result.total_cost_usd || 0;
          isError = result.is_error || false;

          if (result.subtype === 'success') {
            resultText = result.result || '';
          } else {
            isError = true;
            errorMessage = result.errors?.join('; ') || `Agent ended with: ${result.subtype}`;
            resultText = errorMessage ?? '';
          }
          break;
        }
      }
    }
    } catch (iterError: any) {
      // The SDK throws when the process exits with non-zero code, even after
      // yielding a successful result. If we captured a result, use it.
      const msg = iterError?.message || String(iterError);
      if (resultText && !isError) {
        // We have a successful result — the exit code mismatch is harmless
        console.log(`[ClaudeCodeProvider] Process exit error after successful result (ignoring): ${msg}`);
      } else if (!resultText && !isError) {
        // No result captured yet — this is a real failure
        isError = true;
        errorMessage = msg;
        resultText = msg;
      }
      // If isError is already true, we already captured the error from the result message
    }

    const durationMs = Date.now() - startTime;

    // Attach tool calls to the last assistant interaction if any
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
      totalCostUsd,
      numTurns,
      durationMs,
      model,
      error: errorMessage,
    };
  }

  private mapPermissionMode(mode?: string): string {
    switch (mode) {
      case 'acceptEdits': return 'acceptEdits';
      case 'bypassPermissions': return 'bypassPermissions';
      case 'plan': return 'plan';
      case 'dontAsk': return 'dontAsk';
      default: return 'default';
    }
  }

  /**
   * Ensure PATH includes standard binary locations.
   * When the server runs as a nohup daemon, PATH can be minimal.
   * The SDK spawns `node` as a child process and needs to find it.
   *
   * CRITICAL: process.execPath resolves symlinks, so /opt/homebrew/bin/node
   * becomes /opt/homebrew/Cellar/node/25.2.1/bin/node — a directory NOT in
   * the standard PATH dirs. We must include the resolved directory explicitly.
   */
  private ensurePath(currentPath: string): string {
    const nodeDir = path.dirname(process.execPath);
    const requiredDirs = [
      nodeDir,                   // Resolved node binary directory (MUST be first)
      '/opt/homebrew/bin',       // macOS ARM homebrew
      '/usr/local/bin',          // macOS Intel homebrew / standard
      '/usr/bin',                // System binaries
      `${process.env.HOME}/.local/bin`, // User-local (claude CLI)
      `${process.env.HOME}/.nvm/current/bin`, // nvm users
    ];
    const pathDirs = new Set(currentPath.split(':'));
    for (const dir of requiredDirs) {
      if (dir && !pathDirs.has(dir)) {
        pathDirs.add(dir);
      }
    }
    return Array.from(pathDirs).join(':');
  }
}
