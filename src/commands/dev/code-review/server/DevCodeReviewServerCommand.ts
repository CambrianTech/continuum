/**
 * dev/code-review — Shorthand for sentinel/run --template=dev/code-review
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { DevCodeReviewParams, DevCodeReviewResult } from '../shared/DevCodeReviewTypes';
import { Commands } from '../../../../system/core/shared/Commands';

export class DevCodeReviewServerCommand extends CommandBase<DevCodeReviewParams, DevCodeReviewResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('dev/code-review', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<DevCodeReviewResult> {
    const typed = params as JTAGPayload & DevCodeReviewParams;

    const cwd = typed.cwd || process.cwd();

    const templateConfig: Record<string, unknown> = {
      cwd,
      branch: typed.branch ?? 'HEAD',
      baseBranch: typed.baseBranch ?? 'main',
      autonomous: typed.autonomous ?? true,
      roomId: typed.roomId ?? 'general',
      personaId: typed.personaId ?? 'system',
      personaName: typed.personaName ?? 'Code Review',
    };

    try {
      const result = await Commands.execute('sentinel/run', {
        type: 'pipeline',
        template: 'dev/code-review',
        templateConfig,
        async: true,
        parentPersonaId: typed.personaId,
        sentinelName: `dev/code-review — ${typed.branch || 'HEAD'}`,
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
