/**
 * Sentinel Coding Agent Command — Server Implementation
 *
 * Resolves the coding agent provider from the registry, bootstraps a proper
 * workspace via SentinelWorkspaceManager (backed by WorkspaceStrategy),
 * executes the agent, emits progress events via sentinel handle,
 * and captures training data for the LoRA pipeline.
 *
 * Workspace design:
 * - If repoPath is set: SentinelWorkspaceManager creates a project worktree (proper git isolation)
 * - If personaId is set (no repoPath): reuse persona's existing workspace
 * - Otherwise: create a sandbox workspace for the sentinel handle
 *
 * Identity chain fix:
 * - personaId (UUID) is the workspace registration key
 * - sentinelHandle is for event routing only, NOT workspace lookup
 * - Code tools (code/read, code/write) look up workspace by callerId = personaId
 *
 * The agent runs as a CHILD PROCESS (via the SDK) — no main thread blocking.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import { Events } from '../../../../system/core/shared/Events';
import type { SentinelCodingAgentParams, SentinelCodingAgentResult } from '../shared/SentinelCodingAgentTypes';
// Import from barrel — triggers self-registration of built-in providers (ClaudeCodeProvider, etc.)
import { CodingAgentRegistry } from '../../../../system/sentinel/coding-agents';
import type { CodingAgentConfig, CodingAgentProgressEvent } from '../../../../system/sentinel/coding-agents';
import { SentinelWorkspaceManager } from '../../../../system/sentinel/SentinelWorkspaceManager';
import { WorkspaceStrategy } from '../../../../system/code/server/WorkspaceStrategy';
import { CodeDaemon } from '../../../../daemons/code-daemon/shared/CodeDaemon';
import { stringToUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

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

    // ── Bootstrap workspace ──────────────────────────────────────────
    let workspaceCwd: string;
    let workspaceBranch: string | undefined;

    try {
      const wsResult = await this.bootstrapWorkspace(p);
      workspaceCwd = wsResult.cwd;
      workspaceBranch = wsResult.branch;
    } catch (wsErr) {
      return transformPayload(params, {
        success: false,
        error: `Workspace bootstrap failed: ${wsErr instanceof Error ? wsErr.message : String(wsErr)}`,
      });
    }

    // Build config — cwd is the resolved workspace directory, not the raw input
    const config: CodingAgentConfig = {
      prompt: p.prompt,
      cwd: workspaceCwd,
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
        // Include workspace info in result for downstream step interpolation
        workspaceDir: workspaceCwd,
        branch: workspaceBranch,
      });
    } catch (error: unknown) {
      const errorMsg = error instanceof Error ? error.message : String(error);
      return transformPayload(params, {
        success: false,
        error: errorMsg,
      });
    }
  }

  /**
   * Bootstrap a workspace for the coding agent.
   *
   * Three strategies (in priority order):
   * 1. repoPath set → SentinelWorkspaceManager creates a project worktree (git isolation).
   *    Agent codes on branch ai/{persona}/{slug}, main repo untouched.
   * 2. personaId set (no repoPath) → persona already has a workspace (initialized by PersonaUser).
   *    Verify it exists. If not, create a sandbox for the persona UUID.
   * 3. Neither → create a sandbox workspace for the sentinel handle.
   *    Uses the pipeline's workingDir as the root.
   *
   * Returns the resolved workspace directory and branch name.
   */
  private async bootstrapWorkspace(p: SentinelCodingAgentParams): Promise<{ cwd: string; branch?: string }> {
    const handle = p.sentinelHandle ?? 'anon';

    // Strategy 1: Project worktree via SentinelWorkspaceManager (proper git isolation)
    if (p.repoPath && p.personaId) {
      const personaUUID = p.personaId as UUID;
      const workspace = await SentinelWorkspaceManager.acquire({
        sentinelHandle: handle,
        personaId: personaUUID,
        personaUniqueId: `sentinel-${handle}`,
        personaName: p.personaName ?? `Sentinel ${handle}`,
        repoPath: p.repoPath,
        taskSlug: p.taskSlug || 'work',
      });

      return { cwd: workspace.workspaceDir, branch: workspace.branch };
    }

    // Strategy 2: Persona already has a workspace
    if (p.personaId && WorkspaceStrategy.isInitialized(p.personaId)) {
      const project = WorkspaceStrategy.getProjectForPersona(p.personaId);
      if (project) {
        return { cwd: project.worktreeDir, branch: project.branch };
      }
      // Persona exists but no project workspace — use cwd
      return { cwd: p.cwd || process.cwd() };
    }

    // Strategy 3: Sandbox — register the pipeline's workingDir as a bare workspace
    const cwd = p.cwd || process.cwd();
    const workspaceId = p.personaId || p.sentinelHandle || p.userId || 'sentinel-anonymous';
    const jtagRoot = process.cwd();
    const readRoots = cwd !== jtagRoot ? [jtagRoot] : [];
    await CodeDaemon.createWorkspace(workspaceId, cwd, readRoots);
    return { cwd };
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
