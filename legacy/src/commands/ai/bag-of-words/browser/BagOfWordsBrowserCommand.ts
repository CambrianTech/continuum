/**
 * Bag of Words Browser Command
 *
 * Browser implementation - delegates all work to server.
 * Server handles room setup, persona orchestration, and conversation flow.
 */

import { BagOfWordsCommand } from '../shared/BagOfWordsCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { BagOfWordsParams, BagOfWordsResult } from '../shared/BagOfWordsTypes';

export class BagOfWordsBrowserCommand extends BagOfWordsCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('bag-of-words', context, subpath, commander);
  }

  /**
   * Browser just delegates to server for all orchestration
   */
  async execute(params: BagOfWordsParams): Promise<BagOfWordsResult> {
    return await this.remoteExecute(params);
  }
}
