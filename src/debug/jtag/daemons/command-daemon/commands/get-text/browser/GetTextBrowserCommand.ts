/**
 * GetText Command - Browser Implementation
 * 
 * MINIMAL WORK: Uses safeQuerySelector() to find element and extract text
 * Perfect example of focused browser implementation - no over-engineering.
 */

import { GetTextParams, GetTextResult } from '../shared/GetTextTypes';
import { GetTextCommand } from '../shared/GetTextCommand';
import { safeQuerySelector } from '../../../../../shared/GlobalUtils';

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
      
      return new GetTextResult({
        success: true,
        selector: params.selector,
        text: text,
        found: true,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Get text failed:`, error.message);
      return new GetTextResult({
        success: false,
        selector: params.selector,
        text: '',
        found: false,
        error: error.message,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });
    }
  }
}