/**
 * Inference Generate Command - Browser Implementation
 *
 * Generate text using local or cloud AI inference. Auto-routes to best available backend (Candle → Ollama → cloud). Handles model loading, LoRA adapters, and provider failover automatically.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InferenceGenerateParams, InferenceGenerateResult } from '../shared/InferenceGenerateTypes';

export class InferenceGenerateBrowserCommand extends CommandBase<InferenceGenerateParams, InferenceGenerateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('inference/generate', context, subpath, commander);
  }

  async execute(params: InferenceGenerateParams): Promise<InferenceGenerateResult> {
    console.log('🌐 BROWSER: Delegating Inference Generate to server');
    return await this.remoteExecute(params);
  }
}
