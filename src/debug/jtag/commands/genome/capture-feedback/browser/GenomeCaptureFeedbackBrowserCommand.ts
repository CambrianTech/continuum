/**
 * GenomeCaptureFeedbackBrowserCommand - Browser stub
 *
 * All feedback capture happens server-side.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeCaptureFeedbackParams,
  GenomeCaptureFeedbackResult
} from '../shared/GenomeCaptureFeedbackTypes';

export class GenomeCaptureFeedbackBrowserCommand extends CommandBase<
  GenomeCaptureFeedbackParams,
  GenomeCaptureFeedbackResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-capture-feedback', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeCaptureFeedbackResult> {
    // Browser doesn't capture feedback - forward to server
    return transformPayload(params, {
      success: false,
      error: 'Feedback capture must happen on server'
    });
  }
}
