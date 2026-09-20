/**
 * Development Code Smell Command - Browser Implementation
 *
 * Detect code smells: raw Commands.execute, any casts, god classes, missing accessors, type violations. Uses Generator SDK audit + grep patterns.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentCodeSmellParams, DevelopmentCodeSmellResult } from '../shared/DevelopmentCodeSmellTypes';

export class DevelopmentCodeSmellBrowserCommand extends CommandBase<DevelopmentCodeSmellParams, DevelopmentCodeSmellResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/code-smell', context, subpath, commander);
  }

  async execute(params: DevelopmentCodeSmellParams): Promise<DevelopmentCodeSmellResult> {
    console.log('🌐 BROWSER: Delegating Development Code Smell to server');
    return await this.remoteExecute(params);
  }
}
