/**
 * Genome Demo Run Command - Browser Implementation
 *
 * Delegates to server for demo pipeline orchestration.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeDemoRunParams, GenomeDemoRunResult } from '../shared/GenomeDemoRunTypes';

export class GenomeDemoRunBrowserCommand extends CommandBase<GenomeDemoRunParams, GenomeDemoRunResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/demo-run', context, subpath, commander);
  }

  async execute(params: GenomeDemoRunParams): Promise<GenomeDemoRunResult> {
    return await this.remoteExecute(params);
  }
}
