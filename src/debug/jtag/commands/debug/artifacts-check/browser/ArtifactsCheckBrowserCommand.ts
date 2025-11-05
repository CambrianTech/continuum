/**
 * Browser-side artifacts daemon check command (delegates to server)
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ArtifactsCheckParams, ArtifactsCheckResult } from '../shared/ArtifactsCheckTypes';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';

export class ArtifactsCheckBrowserCommand extends CommandBase<ArtifactsCheckParams, ArtifactsCheckResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('artifacts-check', context, subpath, commander);
  }

  async execute(params: ArtifactsCheckParams): Promise<ArtifactsCheckResult> {
    // Browser delegates to server
    return this.remoteExecute(params);
  }
}
