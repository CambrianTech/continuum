// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Navigate Command - Server Implementation
 * 
 * MINIMAL WORK: Server cannot navigate browsers directly, so delegates to
 * browser context via remoteExecute(). Follows screenshot pattern exactly.
 * 
 * DESIGN ANALYSIS:
 * ✅ Single responsibility - delegation only
 * ✅ Proper error handling
 * ✅ Clean result construction 
 * ✅ Uses inherited remoteExecute() method
 * ✅ No unnecessary server-side navigation logic
 * 
 * ARCHITECTURAL FIT:
 * - Perfect example of context-specific behavior
 * - Server knows its limitations and delegates appropriately
 * - Maintains same interface as browser implementation
 * - Elegant simplicity without feature creep
 */

import { type NavigateParams, type NavigateResult, createNavigateResult } from '../shared/NavigateTypes';
import { NetworkError } from '@system/core/types/ErrorTypes';
import { NavigateCommand } from '../shared/NavigateCommand';

export class NavigateServerCommand extends NavigateCommand {
  
  /**
   * Server does ONE thing: delegate to browser for navigation
   */
  async execute(params: NavigateParams): Promise<NavigateResult> {
    console.log(`🖥️ SERVER: Delegating navigation to browser`);

    try {
      // Server just delegates to browser
      return await this.remoteExecute(params);

    } catch (error: any) {
      console.error(`❌ SERVER: Navigation delegation failed:`, error.message);
      const navError = error instanceof Error ? new NetworkError('browser', error.message, { cause: error }) : new NetworkError('browser', String(error));
      return createNavigateResult(params.context, params.sessionId, {
        success: false,
        url: params.url,
        error: navError
      });
    }
  }
}