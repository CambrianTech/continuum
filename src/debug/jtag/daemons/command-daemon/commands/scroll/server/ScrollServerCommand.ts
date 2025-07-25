/**
 * Scroll Command - Server Implementation
 * 
 * MINIMAL WORK PER COMMAND: Just implements what server does
 */

import { CommandBase } from '../../../shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../shared/JTAGTypes';
import type { ScrollParams } from '../shared/ScrollTypes';
import { ScrollResult } from '../shared/ScrollTypes';
import type { ICommandDaemon } from '../../../shared/CommandBase';

export class ScrollServerCommand extends CommandBase<ScrollParams, ScrollResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('scroll', context, subpath, commander);
  }

  /**
   * Server does ONE thing:
   * Delegate to browser for scrolling (can't scroll on server)
   */
  async execute(params: JTAGPayload): Promise<ScrollResult> {
    const scrollParams = params as ScrollParams;
    
    console.log(`📜 SERVER: Starting scroll operation`);

    try {
      // Server always delegates to browser for DOM interaction
      console.log(`🔀 SERVER: Need DOM access → delegating to browser`);
      if (scrollParams.selector) {
        console.log(`📍 SERVER: Scrolling to element "${scrollParams.selector}"`);
      } else {
        console.log(`📍 SERVER: Scrolling to position (${scrollParams.x}, ${scrollParams.y})`);
      }
      
      return await this.remoteExecute(scrollParams);

    } catch (error: any) {
      console.error(`❌ SERVER: Failed:`, error.message);
      return new ScrollResult({
        success: false,
        scrollX: 0,
        scrollY: 0,
        selector: scrollParams.selector,
        scrolled: false,
        environment: this.context.environment,
        timestamp: new Date().toISOString(),
        error: error.message
      });
    }
  }
}