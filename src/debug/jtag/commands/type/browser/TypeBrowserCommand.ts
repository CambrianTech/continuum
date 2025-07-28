/**
 * Type Command - Browser Implementation
 * 
 * MINIMAL WORK: Uses safeQuerySelector() to find element, sets value and dispatches events
 * Perfect example of focused browser implementation - no over-engineering.
 */

import { type TypeParams, type TypeResult, createTypeResult } from '@commandsType/shared/TypeTypes';
import { ValidationError } from '@shared/ErrorTypes';
import { TypeCommand } from '@commandsType/shared/TypeCommand';
import { safeQuerySelector } from '@shared/GlobalUtils';

export class TypeBrowserCommand extends TypeCommand {
  
  /**
   * Browser does ONE thing: type text into element
   */
  async execute(params: TypeParams): Promise<TypeResult> {
    console.log(`⌨️ BROWSER: Typing "${params.text}" into ${params.selector}`);

    try {
      const element = safeQuerySelector(params.selector) as HTMLInputElement;
      if (!element) {
        throw new Error(`Element not found: ${params.selector}`);
      }

      // Clear first if requested
      if (params.clearFirst) {
        element.value = '';
      }

      // Type with optional delay
      if (params.delay && params.delay > 0) {
        // Type character by character with delay
        for (const char of params.text) {
          element.value += char;
          element.dispatchEvent(new Event('input', { bubbles: true }));
          await new Promise(resolve => setTimeout(resolve, params.delay));
        }
      } else {
        // Type all at once
        element.value = params.clearFirst ? params.text : element.value + params.text;
      }

      // Dispatch change event
      element.dispatchEvent(new Event('change', { bubbles: true }));
      
      console.log(`✅ BROWSER: Typed into ${params.selector}`);
      
      return createTypeResult(params.context, params.sessionId, {
        success: true,
        selector: params.selector,
        typed: true,
        text: params.text
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Type failed:`, error.message);
      const typeError = error instanceof Error ? new ValidationError('typing', error.message, { cause: error }) : new ValidationError('typing', String(error));
      return createTypeResult(params.context, params.sessionId, {
        success: false,
        selector: params.selector,
        typed: false,
        text: params.text,
        error: typeError
      });
    }
  }
}