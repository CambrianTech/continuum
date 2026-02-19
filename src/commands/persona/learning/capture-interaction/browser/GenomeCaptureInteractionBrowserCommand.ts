/**
 * GenomeCaptureInteractionBrowserCommand - Browser stub
 *
 * All training data capture happens server-side.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type {
  GenomeCaptureInteractionParams,
  GenomeCaptureInteractionResult
} from '../shared/GenomeCaptureInteractionTypes';

export class GenomeCaptureInteractionBrowserCommand extends CommandBase<
  GenomeCaptureInteractionParams,
  GenomeCaptureInteractionResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/capture-interaction', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeCaptureInteractionResult> {
    // Browser doesn't capture training data - forward to server
    return transformPayload(params, {
      success: false,
      error: 'Training data capture must happen on server'
    });
  }
}
