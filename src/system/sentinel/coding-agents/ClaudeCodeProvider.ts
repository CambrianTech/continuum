/**
 * ClaudeCodeProvider — Claude Agent SDK wrapper implementing CodingAgentProvider.
 *
 * Spawns Claude Code as a child process via the SDK. All heavy work happens
 * in that child process — our event loop just awaits async results.
 *
 * Dynamic import so the SDK is an optional dependency: if not installed,
 * isAvailable() returns false and the system degrades gracefully.
 */

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

    // Stream SDK messages
    const conversation = query({
      prompt: config.prompt,
      options: options as any,
    });

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
            resultText = errorMessage;
          }
          break;
        }
      }
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
}
