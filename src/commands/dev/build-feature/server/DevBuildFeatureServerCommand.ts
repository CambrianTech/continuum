/**
 * dev/build-feature — Shorthand for sentinel/run --template=dev/build-feature
 *
 * Translates CLI-friendly params into sentinel/run template invocation.
 * Auto-detects project type for build/test commands if not specified.
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { DevBuildFeatureParams, DevBuildFeatureResult } from '../shared/DevBuildFeatureTypes';
import { Commands } from '../../../../system/core/shared/Commands';
import { ProjectDetector } from '../../../../system/code/server/ProjectDetector';

export class DevBuildFeatureServerCommand extends CommandBase<DevBuildFeatureParams, DevBuildFeatureResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('dev/build-feature', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<DevBuildFeatureResult> {
    const typed = params as JTAGPayload & DevBuildFeatureParams;

    if (!typed.feature) {
      return transformPayload(params, {
        success: false,
        error: 'Missing required parameter: --feature="description of the feature"',
      });
    }

    const cwd = typed.cwd || process.cwd();
    const project = await ProjectDetector.detect(cwd);

    const templateConfig: Record<string, unknown> = {
      feature: typed.feature,
      cwd,
      autonomous: typed.autonomous ?? true,
      roomId: typed.roomId ?? 'general',
      personaId: typed.personaId ?? 'system',
      personaName: typed.personaName ?? 'Dev Pipeline',
      ...(typed.buildCommand !== undefined && { buildCommand: typed.buildCommand }),
      ...(typed.testCommand !== undefined && { testCommand: typed.testCommand }),
      // Auto-detect if not specified
      ...(typed.buildCommand === undefined && project.buildCommand && { buildCommand: project.buildCommand }),
      ...(typed.testCommand === undefined && project.testCommand && { testCommand: project.testCommand }),
      ...(typed.codingModel && { codingModel: typed.codingModel }),
      ...(typed.maxBudgetUsd && { maxBudgetUsd: typed.maxBudgetUsd }),
    };

    try {
      const result = await Commands.execute('sentinel/run', {
        type: 'pipeline',
        template: 'dev/build-feature',
        templateConfig,
        async: true,
        parentPersonaId: typed.personaId,
        sentinelName: `dev/build-feature — ${typed.feature.slice(0, 60)}`,
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
