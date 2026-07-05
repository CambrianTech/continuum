/**
 * Model Download Command - Browser Implementation
 *
 * Download a base model from HuggingFace to a local or remote grid node. Routes to GPU-capable node if needed. Wraps huggingface_hub snapshot_download with progress reporting via chat.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelDownloadParams, ModelDownloadResult } from '../shared/ModelDownloadTypes';

export class ModelDownloadBrowserCommand extends CommandBase<ModelDownloadParams, ModelDownloadResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/download', context, subpath, commander);
  }

  async execute(params: ModelDownloadParams): Promise<ModelDownloadResult> {
    console.log('🌐 BROWSER: Delegating Model Download to server');
    return await this.remoteExecute(params);
  }
}
