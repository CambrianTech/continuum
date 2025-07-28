/**
 * GetText Command - Browser Implementation
 * 
 * MINIMAL WORK: Uses safeQuerySelector() to find element and extract text
 * Perfect example of focused browser implementation - no over-engineering.
 */

import { type GetTextParams, type GetTextResult, createGetTextResult } from '@commandsGetText/shared/GetTextTypes';
import { ValidationError } from '@shared/ErrorTypes';
import { GetTextCommand } from '@commandsGetText/shared/GetTextCommand';
import { safeQuerySelector } from '@shared/GlobalUtils';

export class GetTextBrowserCommand extends GetTextCommand {
  
  /**
   * Browser does ONE thing: extract text from element
   */
  async execute(params: GetTextParams): Promise<GetTextResult> {
    console.log(`📝 BROWSER: Getting text from ${params.selector}`);

    try {
      const element = safeQuerySelector(params.selector);
      if (!element) {
        throw new Error(`Element not found: ${params.selector}`);
      }

      // Extract text based on preferences
      let text = params.innerText ? (element as HTMLElement).innerText : element.textContent || '';
      
      // Trim if requested
      if (params.trim) {
        text = text.trim();
      }
      
      console.log(`✅ BROWSER: Extracted ${text.length} characters from ${params.selector}`);
      
      return createGetTextResult(params.context, params.sessionId, {
        success: true,
        selector: params.selector,
        text: text,
        found: true
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Get text failed:`, error.message);
      const textError = error instanceof Error ? new ValidationError('selector', error.message, { cause: error }) : new ValidationError('selector', String(error));
      return createGetTextResult(params.context, params.sessionId, {
        success: false,
        selector: params.selector,
        text: '',
        found: false,
        error: textError
      });
    }
  }
}