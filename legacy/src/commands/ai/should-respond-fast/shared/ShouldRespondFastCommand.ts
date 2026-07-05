/**
 * AI Should Respond Fast Command - Shared Base
 *
 * Bag-of-words scoring for response detection
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ShouldRespondFastParams, ShouldRespondFastResult } from './ShouldRespondFastTypes';

export abstract class ShouldRespondFastCommand extends CommandBase<ShouldRespondFastParams, ShouldRespondFastResult> {
  static readonly commandName = 'ai/should-respond-fast';

  getDescription(): string {
    return 'Fast bag-of-words scoring for persona response detection';
  }
}
