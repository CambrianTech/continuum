/**
 * AI Generate Command - Browser Implementation
 * ============================================
 *
 * Browser-specific registration for ai/generate command
 */

import { AIGenerateCommand } from '../shared/AIGenerateCommand';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';

export class AIGenerateBrowserCommand extends AIGenerateCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }
}
