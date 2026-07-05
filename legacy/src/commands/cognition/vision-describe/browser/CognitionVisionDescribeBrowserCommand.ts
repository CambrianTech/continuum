/**
 * Cognition Vision Describe Command - Browser Implementation
 *
 * Describe an image via the best available vision-capable model. Selects a vision-capable model from the Rust model registry, builds the describe prompt from option flags, dispatches `ai/generate` with multimodal content (text + base64 image), and parses the response into a VisionDescription. Migrated from `system/vision/VisionInferenceProvider.ts` per #1276 (oxidizer freeform-shape outlier — pairs with codex's #1284 structured-decision shape). Returns null when no vision model is registered or generation fails.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { CognitionVisionDescribeParams, CognitionVisionDescribeResult } from '../shared/CognitionVisionDescribeTypes';

export class CognitionVisionDescribeBrowserCommand extends CommandBase<CognitionVisionDescribeParams, CognitionVisionDescribeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('cognition/vision-describe', context, subpath, commander);
  }

  async execute(params: CognitionVisionDescribeParams): Promise<CognitionVisionDescribeResult> {
    console.log('🌐 BROWSER: Delegating Cognition Vision Describe to server');
    return await this.remoteExecute(params);
  }
}
