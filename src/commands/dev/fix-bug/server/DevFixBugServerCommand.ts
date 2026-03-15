/**
 * dev/fix-bug — Shorthand for sentinel/run --template=dev/fix-bug
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { DevFixBugParams, DevFixBugResult } from '../shared/DevFixBugTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { ProjectDetector } from '../../../../system/code/server/ProjectDetector';

export class DevFixBugServerCommand extends CommandBase<DevFixBugParams, DevFixBugResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('dev/fix-bug', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<DevFixBugResult> {
    const typed = params as JTAGPayload & DevFixBugParams;

    if (!typed.bug) {
      return transformPayload(params, {
        success: false,
        error: 'Missing required parameter: --bug="description of the bug"',
      });
    }

    const cwd = typed.cwd || process.cwd();
    const project = await ProjectDetector.detect(cwd);

    const templateConfig: Record<string, unknown> = {
      bug: typed.bug,
      cwd,
      autonomous: typed.autonomous ?? true,
      roomId: typed.roomId ?? 'general',
      personaId: typed.personaId ?? 'system',
      personaName: typed.personaName ?? 'Dev Pipeline',
      ...(typed.buildCommand !== undefined && { buildCommand: typed.buildCommand }),
      ...(typed.testCommand !== undefined && { testCommand: typed.testCommand }),
      ...(typed.buildCommand === undefined && project.buildCommand && { buildCommand: project.buildCommand }),
      ...(typed.testCommand === undefined && project.testCommand && { testCommand: project.testCommand }),
      ...(typed.codingModel && { codingModel: typed.codingModel }),
      ...(typed.maxBudgetUsd && { maxBudgetUsd: typed.maxBudgetUsd }),
    };

    try {
      const result = await Commands.execute('sentinel/run', {
        type: 'pipeline',
        template: 'dev/fix-bug',
        templateConfig,
        async: true,
        parentPersonaId: typed.personaId,
        sentinelName: `dev/fix-bug — ${typed.bug.slice(0, 60)}`,
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
