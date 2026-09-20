/**
 * Generate Embedding Command - Browser Implementation
 *
 * Delegates to server-side execution via remoteExecute.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { GenerateEmbeddingParams, GenerateEmbeddingResult } from '../shared/GenerateEmbeddingCommandTypes';

export class GenerateEmbeddingBrowserCommand extends CommandBase<GenerateEmbeddingParams, GenerateEmbeddingResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-generate-embedding', context, subpath, commander);
  }

  async execute(params: GenerateEmbeddingParams): Promise<GenerateEmbeddingResult> {
    return await this.remoteExecute(params);
  }
}
