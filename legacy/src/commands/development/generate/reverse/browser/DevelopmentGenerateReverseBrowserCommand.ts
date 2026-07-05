/**
 * Development Generate Reverse Command - Browser Implementation
 *
 * Reverse-engineer a CommandSpec from an existing hand-written command. Reads the Types file, extracts params, results, command name, and description, then outputs a spec JSON that can be saved to generator/specs/ and used to regenerate the command under generator control.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { DevelopmentGenerateReverseParams, DevelopmentGenerateReverseResult } from '../shared/DevelopmentGenerateReverseTypes';

export class DevelopmentGenerateReverseBrowserCommand extends CommandBase<DevelopmentGenerateReverseParams, DevelopmentGenerateReverseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/generate/reverse', context, subpath, commander);
  }

  async execute(params: DevelopmentGenerateReverseParams): Promise<DevelopmentGenerateReverseResult> {
    console.log('🌐 BROWSER: Delegating Development Generate Reverse to server');
    return await this.remoteExecute(params);
  }
}
