// ISSUES: 0 open, last updated 2025-07-25 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Navigate Command - Browser Implementation
 * 
 * MINIMAL WORK: Just does browser navigation using window.location.
 * Follows screenshot command pattern - browser does browser-specific work,
 * handles errors gracefully, returns typed results.
 * 
 * DESIGN ANALYSIS:
 * ✅ Focused implementation - only browser navigation logic
 * ✅ Proper error handling with try/catch
 * ✅ Clean result object construction
 * ✅ Appropriate console logging for debugging
 * ✅ Optional selector waiting without over-engineering
 * 
 * ARCHITECTURAL FIT:
 * - Extends NavigateCommand abstract base
 * - Uses NavigateParams/NavigateResult types
 * - No dependencies beyond what's needed
 * - Clean, readable implementation
 */

import { type NavigateParams, type NavigateResult, createNavigateResult } from '../shared/NavigateTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { NavigateCommand } from '../shared/NavigateCommand';

export class NavigateBrowserCommand extends NavigateCommand {
  
  /**
   * Browser navigation - navigates to URL or reloads if no URL provided
   */
  async execute(params: NavigateParams): Promise<NavigateResult> {
    const isReload = !params.url;
    console.log(isReload ? '🔄 BROWSER: Reloading page' : `🌐 BROWSER: Navigating to ${params.url}`);

    try {
      const startTime = Date.now();

      if (isReload) {
        // No URL provided - reload current page
        window.location.reload();
      } else {
        // Navigate to specified URL
        window.location.href = params.url;
      }

      // Wait for load if requested
      if (params.waitForSelector) {
        await this.waitForElement(params.waitForSelector, params.timeout || 5000);
      }

      const loadTime = Date.now() - startTime;
      console.log(`✅ BROWSER: ${isReload ? 'Reloaded' : 'Navigated'} in ${loadTime}ms`);

      return createNavigateResult(params.context, params.sessionId, {
        success: true,
        url: window.location.href,
        title: document.title,
        loadTime
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: ${isReload ? 'Reload' : 'Navigation'} failed:`, error.message);
      const navError = error instanceof Error ? new ValidationError('url', error.message, { cause: error }) : new ValidationError('url', String(error));
      return createNavigateResult(params.context, params.sessionId, {
        success: false,
        url: params.url || window.location.href,
        error: navError
      });
    }
  }

  private async waitForElement(selector: string, timeout: number): Promise<void> {
    return new Promise((resolve, reject) => {
      const startTime = Date.now();
      
      const check = () => {
        if (document.querySelector(selector)) {
          resolve();
        } else if (Date.now() - startTime > timeout) {
          reject(new Error(`❌ TIMEOUT: Element selector '${selector}' not found within ${timeout}ms - navigation cancelled`));
        } else {
          setTimeout(check, 100);
        }
      };
      
      check();
    });
  }
}