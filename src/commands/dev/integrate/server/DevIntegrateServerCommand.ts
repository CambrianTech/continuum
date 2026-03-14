/**
 * dev/integrate — Shorthand for sentinel/run --template=dev/integrate
 *
 * Merges persona branches into a shared feature branch, resolving
 * conflicts with CodingAgent and running build+test after each merge.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { DevIntegrateParams, DevIntegrateResult } from '../shared/DevIntegrateTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { detectProject } from '../../../../system/sentinel/ProjectDetector';

export class DevIntegrateServerCommand extends CommandBase<DevIntegrateParams, DevIntegrateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('dev/integrate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<DevIntegrateResult> {
    const typed = params as JTAGPayload & DevIntegrateParams;

    if (!typed.featureBranch) {
      return transformPayload(params, {
        success: false,
        error: 'Missing required parameter: --featureBranch="feature/branch-name"',
      });
    }

    const cwd = typed.cwd || process.cwd();
    const project = detectProject(cwd);

    // Parse comma-separated branches into array
    const branches = typed.branches
      ? typed.branches.split(',').map(b => b.trim()).filter(b => b)
      : undefined;

    const templateConfig: Record<string, unknown> = {
      featureBranch: typed.featureBranch,
      cwd,
      autonomous: typed.autonomous ?? true,
      roomId: typed.roomId ?? 'general',
      personaId: typed.personaId ?? 'system',
      personaName: typed.personaName ?? 'Integration Lead',
      ...(branches && { branches }),
      ...(typed.baseBranch && { baseBranch: typed.baseBranch }),
      ...(typed.buildCommand !== undefined && { buildCommand: typed.buildCommand }),
      ...(typed.testCommand !== undefined && { testCommand: typed.testCommand }),
      ...(typed.buildCommand === undefined && project.buildCommand !== null && { buildCommand: project.buildCommand }),
      ...(typed.testCommand === undefined && project.testCommand !== null && { testCommand: project.testCommand }),
      ...(typed.codingModel && { codingModel: typed.codingModel }),
      ...(typed.maxBudgetUsd && { maxBudgetUsd: typed.maxBudgetUsd }),
    };

    try {
      const result = await Commands.execute('sentinel/run', {
        type: 'pipeline',
        template: 'dev/integrate',
        templateConfig,
        async: true,
        parentPersonaId: typed.personaId,
        sentinelName: `dev/integrate — ${typed.featureBranch}`,
      } as Record<string, unknown>);

      const runResult = result as unknown as Record<string, unknown>;
      return transformPayload(params, {
        success: runResult.success as boolean,
        handle: runResult.handle as string | undefined,
        error: runResult.error as string | undefined,
      });
    } catch (err) {
      return transformPayload(params, {
        success: false,
        error: `Failed to launch: ${err instanceof Error ? err.message : String(err)}`,
      });
    }
  }
}
