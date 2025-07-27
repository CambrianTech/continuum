/**
 * GetText Command - Server Implementation
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import { type GetTextParams, GetTextResult } from '../shared/GetTextTypes';

export class GetTextServerCommand extends CommandBase<GetTextParams, GetTextResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('get-text', context, subpath, commander);
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
      return new GetTextResult({
        success: false,
        selector: getTextParams.selector,
        text: '',
        found: false,
        environment: this.context.environment,
        timestamp: new Date().toISOString(),
        error: error.message
      }, getTextParams.context, getTextParams.sessionId);
    }
  }
}