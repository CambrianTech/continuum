/**
 * Development Build Command - Browser Implementation
 *
 * Zero-friction TypeScript build check. Returns success or structured errors.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentBuildParams, DevelopmentBuildResult } from '../shared/DevelopmentBuildTypes';

export class DevelopmentBuildBrowserCommand extends CommandBase<DevelopmentBuildParams, DevelopmentBuildResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/build', context, subpath, commander);
  }

  async execute(params: DevelopmentBuildParams): Promise<DevelopmentBuildResult> {
    console.log('🌐 BROWSER: Delegating Development Build to server');
    return await this.remoteExecute(params);
  }
}
