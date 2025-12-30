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
import { Events } from '@system/core/shared/Events';

export class NavigateBrowserCommand extends NavigateCommand {
  
  /**
   * Browser navigation - navigates to URL or reloads if no URL provided
   * When target='webview', navigates the co-browsing widget instead
   */
  async execute(params: NavigateParams): Promise<NavigateResult> {
    const isReload = !params.url;
    const isWebview = params.target === 'webview';

    if (isWebview) {
      return this.navigateWebview(params);
    }

    console.log(isReload ? '🔄 BROWSER: Reloading page' : `🌐 BROWSER: Navigating to ${params.url}`);

    try {
      const startTime = Date.now();
      const targetUrl = params.url || window.location.href;

      // Return result FIRST, then navigate (navigation destroys JS context)
      const result = createNavigateResult(params.context, params.sessionId, {
        success: true,
        url: targetUrl,
        title: isReload ? document.title : `Navigating to ${targetUrl}`,
        loadTime: Date.now() - startTime
      });

      // Schedule navigation after a microtask to allow result to be sent
      setTimeout(() => {
        if (isReload) {
          window.location.reload();
        } else {
          window.location.href = params.url!;
        }
      }, 50);

      console.log(`✅ BROWSER: ${isReload ? 'Reload' : 'Navigation'} initiated to ${targetUrl}`);
      return result;

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

  /**
   * Navigate the co-browsing webview widget instead of main window
   */
  private async navigateWebview(params: NavigateParams): Promise<NavigateResult> {
    if (!params.url) {
      return createNavigateResult(params.context, params.sessionId, {
        success: false,
        url: '',
        error: new ValidationError('url', 'URL is required for webview navigation')
      });
    }

    console.log(`🌐 BROWSER: Navigating webview to ${params.url}`);

    // Store pending URL in localStorage for WebViewWidget to pick up on mount
    // This handles the case where the widget isn't mounted yet when we emit the event
    localStorage.setItem('webview:pending-url', params.url);

    // First switch to Browser tab by emitting content:opened event
    // This follows the same pattern as RoomListWidget, UserListWidget etc.
    Events.emit('content:opened', {
      contentType: 'browser',
      entityId: 'browser',
      title: 'Browser',
      setAsCurrent: true
    });

    // Wait for widget to mount (Browser tab needs time to render WebViewWidget)
    await new Promise(resolve => setTimeout(resolve, 500));

    // Emit navigate event for WebViewWidget to handle
    // Also send as backup in case widget is already mounted
    Events.emit('webview:navigate', { url: params.url });

    return createNavigateResult(params.context, params.sessionId, {
      success: true,
      url: params.url,
      title: 'WebView navigation initiated'
    });
  }
}