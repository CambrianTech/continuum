/**
 * GetText Command - Server Implementation
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { type GetTextParams, type GetTextResult, createGetTextResult } from '../shared/GetTextTypes';
import { NetworkError } from '@system/core/types/ErrorTypes';

export class GetTextServerCommand extends CommandBase<GetTextParams, GetTextResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/get-text', context, subpath, commander);
  }

  /**
   * Server does ONE thing:
   * Delegate to browser for DOM text extraction (can't access DOM on server)
   */
  async execute(params: JTAGPayload): Promise<GetTextResult> {
    const getTextParams = params as GetTextParams;
    
    console.log(`📝 SERVER: Starting get-text operation`);

    try {
      // Server always delegates to browser for DOM interaction
      console.log(`🔀 SERVER: Need DOM access → delegating to browser`);
      console.log(`🔍 SERVER: Getting text from "${getTextParams.selector}"`);
      
      return await this.remoteExecute(getTextParams);

    } catch (error: any) {
      console.error(`❌ SERVER: Failed:`, error.message);
      const textError = error instanceof Error ? new NetworkError('browser', error.message, { cause: error }) : new NetworkError('browser', String(error));
      return createGetTextResult(getTextParams.context, getTextParams.sessionId, {
        success: false,
        selector: getTextParams.selector,
        text: '',
        found: false,
        error: textError
      });
    }
  }
}