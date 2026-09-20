/**
 * Genome Training Pipeline Command - Browser Implementation
 *
 * One-command entry point for full LoRA training workflow. Builds a Sentinel pipeline that prepares data, trains adapter, registers it, and activates it for a persona
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeTrainingPipelineParams, GenomeTrainingPipelineResult } from '../shared/GenomeTrainingPipelineTypes';

export class GenomeTrainingPipelineBrowserCommand extends CommandBase<GenomeTrainingPipelineParams, GenomeTrainingPipelineResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/training-pipeline', context, subpath, commander);
  }

  async execute(params: GenomeTrainingPipelineParams): Promise<GenomeTrainingPipelineResult> {
    console.log('🌐 BROWSER: Delegating Genome Training Pipeline to server');
    return await this.remoteExecute(params);
  }
}
