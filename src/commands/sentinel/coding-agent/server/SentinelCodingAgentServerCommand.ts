/**
 * Sentinel Coding Agent Command — Server Implementation
 *
 * Resolves the coding agent provider from the registry, executes the agent,
 * emits progress events via sentinel handle, and captures training data
 * for the LoRA pipeline when personaId is set.
 *
 * The agent runs as a CHILD PROCESS (via the SDK) — no main thread blocking.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Events } from '../../../../system/core/shared/Events';
import type { SentinelCodingAgentParams, SentinelCodingAgentResult } from '../shared/SentinelCodingAgentTypes';
import { CodingAgentRegistry } from '../../../../system/sentinel/coding-agents/CodingAgentRegistry';
import type { CodingAgentConfig, CodingAgentProgressEvent } from '../../../../system/sentinel/coding-agents/CodingAgentProvider';

export class SentinelCodingAgentServerCommand extends CommandBase<SentinelCodingAgentParams, SentinelCodingAgentResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('sentinel/coding-agent', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SentinelCodingAgentResult> {
    const p = params as SentinelCodingAgentParams;

    if (!p.prompt) {
      return transformPayload(params, {
        success: false,
        error: 'Missing required "prompt" parameter',
      });
    }

    const providerId = p.provider || 'claude-code';
    const provider = CodingAgentRegistry.get(providerId);

    if (!provider) {
      const available = CodingAgentRegistry.providerIds;
      return transformPayload(params, {
        success: false,
        error: `Unknown provider "${providerId}". Available: ${available.join(', ') || 'none (install @anthropic-ai/claude-agent-sdk)'}`,
      });
    }

    // Check provider availability (SDK installed?)
    const available = await provider.isAvailable();
    if (!available) {
      return transformPayload(params, {
        success: false,
        error: `Provider "${providerId}" is not available. Ensure its SDK is installed.`,
      });
    }

    // Build config
    const config: CodingAgentConfig = {
      prompt: p.prompt,
      cwd: p.cwd || process.cwd(),
      systemPrompt: p.systemPrompt,
      model: p.model,
      allowedTools: p.allowedTools,
      maxTurns: p.maxTurns,
      maxBudgetUsd: p.maxBudgetUsd,
      permissionMode: p.permissionMode,
      resumeSessionId: p.resumeSessionId,
      sentinelHandle: p.sentinelHandle,
      captureTraining: p.captureTraining,
      personaId: p.personaId,
    };

    // Progress callback — emits sentinel events
    const onProgress = (event: CodingAgentProgressEvent): void => {
      if (p.sentinelHandle) {
        Events.emit(`sentinel:${p.sentinelHandle}:progress`, {
          handle: p.sentinelHandle,
          stepType: 'codingagent',
          provider: providerId,
          ...event,
        });
      }
    };

    try {
      const result = await provider.execute(config, onProgress);

      // Capture training data if enabled
      if (this.shouldCaptureTraining(p) && result.interactions.length > 0) {
        await this.captureTrainingData(p.personaId!, result);
      }

      return transformPayload(params, {
        success: result.success,
        text: result.text,
        agentSessionId: result.sessionId,
        toolCalls: result.toolCalls,
        interactions: result.interactions,
        totalCostUsd: result.totalCostUsd,
        numTurns: result.numTurns,
        durationMs: result.durationMs,
        model: result.model,
        error: result.error,
      });
    } catch (error: any) {
      return transformPayload(params, {
        success: false,
        error: error.message || String(error),
      });
    }
  }

  private shouldCaptureTraining(params: SentinelCodingAgentParams): boolean {
    if (params.captureTraining === false) return false;
    if (params.captureTraining === true) return !!params.personaId;
    // Default: capture if personaId is set
    return !!params.personaId;
  }

  private async captureTrainingData(
    personaId: string,
    result: { interactions: Array<{ role: string; content: string }>; success: boolean },
  ): Promise<void> {
    try {
      const { GenomeCaptureInteraction } = await import(
        '../../../../commands/persona/learning/capture-interaction/shared/GenomeCaptureInteractionTypes'
      );

      for (let i = 0; i < result.interactions.length - 1; i++) {
        const current = result.interactions[i];
        const next = result.interactions[i + 1];

        if (current.role === 'user' && next.role === 'assistant') {
          await GenomeCaptureInteraction.execute({
            roleId: personaId,
            personaId,
            domain: 'coding',
            input: current.content,
            output: next.content,
            metadata: {
              source: 'coding-agent',
              quality: result.success ? 0.9 : 0.3,
            },
          });
        }
      }
    } catch (error) {
      // Training capture is best-effort — don't fail the command
      console.warn('[sentinel/coding-agent] Training capture failed:', error);
    }
  }
}
