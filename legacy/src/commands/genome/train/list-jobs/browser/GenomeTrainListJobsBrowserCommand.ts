/**
 * Genome Train List Jobs Command - Browser Implementation
 *
 * List all training jobs with status, progress, checkpoints, and node info. Shows running, completed, crashed, and resumable jobs. Use genome/train/resume to restart crashed jobs from their latest checkpoint.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainListJobsParams, GenomeTrainListJobsResult } from '../shared/GenomeTrainListJobsTypes';

export class GenomeTrainListJobsBrowserCommand extends CommandBase<GenomeTrainListJobsParams, GenomeTrainListJobsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/train/list-jobs', context, subpath, commander);
  }

  async execute(params: GenomeTrainListJobsParams): Promise<GenomeTrainListJobsResult> {
    console.log('🌐 BROWSER: Delegating Genome Train List Jobs to server');
    return await this.remoteExecute(params);
  }
}
