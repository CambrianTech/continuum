// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Click Command - Server Implementation
 * 
 * MINIMAL WORK: Server cannot click DOM elements, so delegates to browser.
 * Perfect example of context-aware behavior - knows its limitations.
 * 
 * DESIGN ANALYSIS:
 * ✅ Single responsibility - delegation only
 * ✅ Clean use of inherited remoteExecute()
 * ✅ Proper error handling and result construction
 * ✅ No attempt at server-side DOM manipulation
 * ✅ Maintains interface consistency with browser impl
 * 
 * ARCHITECTURAL INSIGHT:
 * - Server implementations often just delegate
 * - This is elegant, not lazy - proper separation of concerns
 * - Same interface everywhere, different behavior per context
 * - No god objects or feature creep
 */

import { type ClickParams, type ClickResult, createClickResult } from '../shared/ClickTypes';
import { NetworkError } from '@system/core/types/ErrorTypes';
import { ClickCommand } from '../shared/ClickCommand';

export class ClickServerCommand extends ClickCommand {
  
  /**
   * Server does ONE thing: delegate to browser
   */
  async execute(params: ClickParams): Promise<ClickResult> {
    console.log(`🖥️ SERVER: Delegating click to browser`);

    try {
      return await this.remoteExecute(params);

    } catch (error: any) {
      console.error(`❌ SERVER: Click delegation failed:`, error.message);
      const clickError = error instanceof Error ? new NetworkError('browser', error.message, { cause: error }) : new NetworkError('browser', String(error));
      return createClickResult(params.context, params.sessionId, {
        success: false,
        selector: params.selector,
        clicked: false,
        error: clickError
      });
    }
  }
}