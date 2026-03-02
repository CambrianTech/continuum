/**
 * Genome Convert Command - Browser Implementation
 *
 * Convert LoRA adapters between formats. Supports: merge LoRA into full-precision model, merge + quantize to GGUF, quantize base model to GGUF, and validate converted models. Uses convert-adapter.py via Rust sentinel for process isolation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { GenomeConvertParams, GenomeConvertResult } from '../shared/GenomeConvertTypes';

export class GenomeConvertBrowserCommand extends CommandBase<GenomeConvertParams, GenomeConvertResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/convert', context, subpath, commander);
  }

  async execute(params: GenomeConvertParams): Promise<GenomeConvertResult> {
    console.log('🌐 BROWSER: Delegating Genome Convert to server');
    return await this.remoteExecute(params);
  }
}
