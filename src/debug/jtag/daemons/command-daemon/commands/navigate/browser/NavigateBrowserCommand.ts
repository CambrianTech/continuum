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

import { NavigateParams, NavigateResult } from '../shared/NavigateTypes';
import { NavigateCommand } from '../shared/NavigateCommand';

export class NavigateBrowserCommand extends NavigateCommand {
  
  /**
   * Browser does ONE thing: navigate to URL
   */
  async execute(params: NavigateParams): Promise<NavigateResult> {
    console.log(`🌐 BROWSER: Navigating to ${params.url}`);

    try {
      const startTime = Date.now();
      
      // Simple browser navigation
      window.location.href = params.url;
      
      // Wait for load if requested
      if (params.waitForSelector) {
        await this.waitForElement(params.waitForSelector, params.timeout || 5000);
      }
      
      const loadTime = Date.now() - startTime;
      console.log(`✅ BROWSER: Navigated in ${loadTime}ms`);
      
      return new NavigateResult({
        success: true,
        url: window.location.href,
        title: document.title,
        loadTime,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Navigation failed:`, error.message);
      return new NavigateResult({
        success: false,
        url: params.url,
        error: error.message,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
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
          reject(new Error(`Element ${selector} not found within ${timeout}ms`));
        } else {
          setTimeout(check, 100);
        }
      };
      
      check();
    });
  }
}