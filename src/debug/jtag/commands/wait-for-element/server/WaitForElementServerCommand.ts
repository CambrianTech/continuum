/**
 * WaitForElement Command - Server Implementation
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import type { WaitForElementParams } from '@commandsWaitForElement/shared/WaitForElementTypes';
import { type WaitForElementResult, createWaitForElementResult } from '@commandsWaitForElement/shared/WaitForElementTypes';
import { NetworkError } from '@shared/ErrorTypes';

export class WaitForElementServerCommand extends CommandBase<WaitForElementParams, WaitForElementResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('wait-for-element', context, subpath, commander);
  }

  /**
   * Server does ONE thing:
   * Delegate to browser for DOM waiting (can't wait for elements on server)
   */
  async execute(params: JTAGPayload): Promise<WaitForElementResult> {
    const waitParams = params as WaitForElementParams;
    
    console.log(`⏳ SERVER: Starting wait-for-element operation`);

    try {
      // Server always delegates to browser for DOM interaction
      console.log(`🔀 SERVER: Need DOM access → delegating to browser`);
      console.log(`👀 SERVER: Waiting for "${waitParams.selector}" (timeout: ${waitParams.timeout}ms)`);
      
      return await this.remoteExecute(waitParams);

    } catch (error: any) {
      console.error(`❌ SERVER: Failed:`, error.message);
      const waitError = error instanceof Error ? new NetworkError('browser', error.message, { cause: error }) : new NetworkError('browser', String(error));
      return createWaitForElementResult(waitParams.context, waitParams.sessionId, {
        success: false,
        selector: waitParams.selector,
        found: false,
        visible: false,
        timeout: waitParams.timeout || 30000,
        waitTime: 0,
        error: waitError
      });
    }
  }
}