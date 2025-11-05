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

import { type ClickParams, type ClickResult, createClickResult } from '../shared/ClickTypes';
import { ValidationError } from '../../../system/core/types/ErrorTypes';
import { ClickCommand } from '../shared/ClickCommand';
import { safeQuerySelector } from '../../../daemons/command-daemon/shared/GlobalUtils';
import { WidgetDiscovery } from '../../../system/core/browser/utils/WidgetIntrospection';

export class ClickBrowserCommand extends ClickCommand {
  
  /**
   * Browser does ONE thing: click element
   * Handles both regular selectors and widget selectors (with shadow DOM traversal)
   */
  async execute(params: ClickParams): Promise<ClickResult> {
    console.log(`👆 BROWSER: Clicking ${params.selector}`);

    try {
      let element: Element | null = null;
      let clickTarget: Element | null = null;

      // Check if selector looks like a widget (ends with -widget)
      if (params.selector.endsWith('-widget')) {
        console.log(`🔍 BROWSER: Widget selector detected, using WidgetDiscovery`);
        const widgetRef = WidgetDiscovery.findWidget(params.selector);

        if (!widgetRef) {
          throw new Error(`Widget not found: ${params.selector}`);
        }

        element = widgetRef.element;

        // If shadowRoot and innerSelector provided, find element inside widget's shadow DOM
        if (params.shadowRoot && params.innerSelector && widgetRef.shadowRoot) {
          clickTarget = widgetRef.shadowRoot.querySelector(params.innerSelector);
          if (!clickTarget) {
            throw new Error(`Inner element not found: ${params.innerSelector} inside ${params.selector}`);
          }
          console.log(`🎯 BROWSER: Found inner element ${params.innerSelector} inside widget`);
        } else {
          clickTarget = element;
        }
      } else {
        // Regular selector (non-widget)
        element = safeQuerySelector(params.selector);
        if (!element) {
          throw new Error(`Element not found: ${params.selector}`);
        }
        clickTarget = element;
      }

      // Click the target element
      (clickTarget as HTMLElement).click();

      console.log(`✅ BROWSER: Clicked ${params.selector}${params.innerSelector ? ` -> ${params.innerSelector}` : ''}`);

      return createClickResult(params.context, params.sessionId, {
        success: true,
        selector: params.selector,
        clicked: true
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Click failed:`, error.message);
      const clickError = error instanceof Error ? new ValidationError('selector', error.message, { cause: error }) : new ValidationError('selector', String(error));
      return createClickResult(params.context, params.sessionId, {
        success: false,
        selector: params.selector,
        clicked: false,
        error: clickError
      });
    }
  }
}