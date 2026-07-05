/**
 * Genome Train Resume Command - Browser Implementation
 *
 * Resume a crashed or failed training job from its latest checkpoint. Looks up the TrainingJobEntity, verifies checkpoint exists on disk, and restarts genome/train with resumeFromCheckpoint pointing to the latest checkpoint directory.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainResumeParams, GenomeTrainResumeResult } from '../shared/GenomeTrainResumeTypes';

export class GenomeTrainResumeBrowserCommand extends CommandBase<GenomeTrainResumeParams, GenomeTrainResumeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/train/resume', context, subpath, commander);
  }

  async execute(params: GenomeTrainResumeParams): Promise<GenomeTrainResumeResult> {
    console.log('🌐 BROWSER: Delegating Genome Train Resume to server');
    return await this.remoteExecute(params);
  }
}
