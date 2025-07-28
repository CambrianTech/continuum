/**
 * Type Command - Server Implementation
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import { type TypeParams, type TypeResult, createTypeResult } from '@commandsType/shared/TypeTypes';
import { NetworkError } from '@shared/ErrorTypes';

export class TypeServerCommand extends CommandBase<TypeParams, TypeResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('type', context, subpath, commander);
  }

  /**
   * Server does ONE thing:
   * Delegate to browser for text input (can't type on server)
   */
  async execute(params: JTAGPayload): Promise<TypeResult> {
    const typeParams = params as TypeParams;
    
    console.log(`⌨️ SERVER: Starting type operation`);

    try {
      // Server always delegates to browser for DOM interaction
      console.log(`🔀 SERVER: Need DOM access → delegating to browser`);
      console.log(`📝 SERVER: Typing "${typeParams.text}" into selector "${typeParams.selector}"`);
      
      return await this.remoteExecute(typeParams);

    } catch (error: any) {
      console.error(`❌ SERVER: Failed:`, error.message);
      const typeError = error instanceof Error ? new NetworkError('browser', error.message, { cause: error }) : new NetworkError('browser', String(error));
      return createTypeResult(typeParams.context, typeParams.sessionId, {
        success: false,
        selector: typeParams.selector,
        typed: false,
        text: typeParams.text,
        error: typeError
      });
    }
  }
}