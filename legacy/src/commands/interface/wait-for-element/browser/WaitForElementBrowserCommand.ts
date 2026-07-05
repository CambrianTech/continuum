/**
 * WaitForElement Command - Browser Implementation
 * 
 * MINIMAL WORK: Polls for element existence and visibility with timeout
 * Perfect example of focused browser implementation - no over-engineering.
 */

import { type WaitForElementParams, type WaitForElementResult, createWaitForElementResult } from '../shared/WaitForElementTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import { WaitForElementCommand } from '../shared/WaitForElementCommand';
import { safeQuerySelector } from '@daemons/command-daemon/shared/GlobalUtils';

export class WaitForElementBrowserCommand extends WaitForElementCommand {
  
  /**
   * Browser does ONE thing: wait for element to appear/be visible
   */
  async execute(params: WaitForElementParams): Promise<WaitForElementResult> {
    console.log(`⏳ BROWSER: Waiting for ${params.selector} (timeout: ${params.timeout}ms)`);

    const startTime = Date.now();
    const timeout = params.timeout || 30000;
    const interval = params.interval || 100;

    try {
      while (Date.now() - startTime < timeout) {
        const element = safeQuerySelector(params.selector);
        
        if (element) {
          // Element found - check visibility if required
          if (!params.visible) {
            // Just existence check
            console.log(`✅ BROWSER: Element found: ${params.selector}`);
            return createWaitForElementResult(params.context, params.sessionId, {
              success: true,
              selector: params.selector,
              found: true,
              visible: false, // Not checking visibility
              timeout: timeout,
              waitTime: Date.now() - startTime,
                  });
          }
          
          // Check if element is visible
          const rect = element.getBoundingClientRect();
          const isVisible = rect.width > 0 && rect.height > 0 && 
                           window.getComputedStyle(element).visibility !== 'hidden' &&
                           window.getComputedStyle(element).display !== 'none';
          
          if (isVisible) {
            console.log(`✅ BROWSER: Element visible: ${params.selector}`);
            return createWaitForElementResult(params.context, params.sessionId, {
              success: true,
              selector: params.selector,
              found: true,
              visible: true,
              timeout: timeout,
              waitTime: Date.now() - startTime,
                  });
          }
        }
        
        // Wait before next check
        await new Promise(resolve => setTimeout(resolve, interval));
      }
      
      // Timeout reached
      throw new Error(`Timeout waiting for element: ${params.selector}`);

    } catch (error: any) {
      console.error(`❌ BROWSER: Wait failed:`, error.message);
      const waitError = error instanceof Error ? new ValidationError('wait', error.message, { cause: error }) : new ValidationError('wait', String(error));
      return createWaitForElementResult(params.context, params.sessionId, {
        success: false,
        selector: params.selector,
        found: false,
        visible: false,
        timeout: timeout,
        waitTime: Date.now() - startTime,
        error: waitError
      });
    }
  }
}