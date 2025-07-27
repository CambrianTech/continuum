// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Click Command - Browser Implementation
 * 
 * MINIMAL WORK: Uses safeQuerySelector() to find element, calls .click().
 * Perfect example of focused browser implementation - no over-engineering.
 * 
 * DESIGN ANALYSIS:
 * ✅ Single responsibility - just element clicking
 * ✅ Uses shared GlobalUtils.safeQuerySelector()
 * ✅ Proper error handling with meaningful messages
 * ✅ Clean result object construction
 * ✅ Appropriate console logging for debugging
 * ✅ No unnecessary complexity or features
 * 
 * ARCHITECTURAL FIT:
 * - Follows screenshot pattern exactly
 * - Uses established utilities (safeQuerySelector)
 * - Browser does browser work, nothing else
 * - Clean, readable, maintainable
 */

import { type ClickParams, type ClickResult, createClickResult } from '@clickShared/ClickTypes';
import { ClickCommand } from '@clickShared/ClickCommand';
import { safeQuerySelector } from '@shared/GlobalUtils';

export class ClickBrowserCommand extends ClickCommand {
  
  /**
   * Browser does ONE thing: click element
   */
  async execute(params: ClickParams): Promise<ClickResult> {
    console.log(`👆 BROWSER: Clicking ${params.selector}`);

    try {
      const element = safeQuerySelector(params.selector);
      if (!element) {
        throw new Error(`Element not found: ${params.selector}`);
      }

      // Simple click (cast to HTMLElement for click method)
      (element as HTMLElement).click();
      
      console.log(`✅ BROWSER: Clicked ${params.selector}`);
      
      return createClickResult(params.context, params.sessionId, {
        success: true,
        selector: params.selector,
        clicked: true
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Click failed:`, error.message);
      return createClickResult(params.context, params.sessionId, {
        success: false,
        selector: params.selector,
        clicked: false,
        error: error.message
      });
    }
  }
}