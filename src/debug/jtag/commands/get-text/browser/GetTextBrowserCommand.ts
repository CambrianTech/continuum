/**
 * GetText Command - Browser Implementation
 * 
 * MINIMAL WORK: Uses safeQuerySelector() to find element and extract text
 * Perfect example of focused browser implementation - no over-engineering.
 */

import { type GetTextParams, type GetTextResult, createGetTextResult } from '../shared/GetTextTypes';
import { ValidationError } from '../../../system/core/types/ErrorTypes';
import { GetTextCommand } from '../shared/GetTextCommand';
import { safeQuerySelector } from '../../../daemons/command-daemon/shared/GlobalUtils';
import { ShadowDOMQueryOptions } from '../../../system/browser/WidgetUtils';
import { ShadowDOMBrowserQuery } from './ShadowDOMBrowserQuery';

export class GetTextBrowserCommand extends GetTextCommand {
  
  /**
   * Browser does ONE thing: extract text from element (WITH SHADOW DOM SUPPORT)
   */
  async execute(params: GetTextParams): Promise<GetTextResult> {
    console.log(`📝 BROWSER: Getting text from ${params.selector} (Shadow DOM aware)`);

    try {
      // Try Shadow DOM-aware extraction first
      const queryOptions: ShadowDOMQueryOptions = {
        querySelector: params.selector,
        includeMainDocument: true,
        includeShadowContent: false, // We want element text, not raw HTML
        maxDepth: 10
      };
      
      // Execute the Shadow DOM query using proper TypeScript
      const shadowSearchResult = ShadowDOMBrowserQuery.searchShadowDOM(queryOptions);
      const shadowTextResult = ShadowDOMBrowserQuery.extractTextFromResults(shadowSearchResult);
      
      if (shadowTextResult.found && shadowTextResult.text) {
        let text = shadowTextResult.text;
        
        // Apply text preferences
        if (params.trim) {
          text = text.trim();
        }
        
        console.log(`✅ BROWSER: Extracted ${text.length} characters from Shadow DOM (${params.selector})`);
        console.log(`🔍 BROWSER: Found in ${shadowSearchResult.totalShadowRoots} shadow roots`);
        
        return createGetTextResult(params.context, params.sessionId, {
          success: true,
          selector: params.selector,
          text: text,
          found: true,
          shadowDOMData: shadowSearchResult
        });
      }
      
      // Fallback to standard query selector
      console.log(`🔍 BROWSER: Shadow DOM search failed, falling back to standard selector`);
      const element = safeQuerySelector(params.selector);
      if (!element) {
        throw new Error(`Element not found in both Shadow DOM and main document: ${params.selector}`);
      }

      // Extract text based on preferences
      let text = params.innerText ? (element as HTMLElement).innerText : element.textContent || '';
      
      // Trim if requested
      if (params.trim) {
        text = text.trim();
      }
      
      console.log(`✅ BROWSER: Extracted ${text.length} characters from main document (${params.selector})`);
      
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