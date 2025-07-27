/**
 * Scroll Command - Browser Implementation
 * 
 * MINIMAL WORK: Scrolls window or element using smooth scrolling
 * Perfect example of focused browser implementation - no over-engineering.
 */

import { type ScrollParams, type ScrollResult, createScrollResult } from '../shared/ScrollTypes';
import { ValidationError } from '@shared/ErrorTypes';
import { ScrollCommand } from '../shared/ScrollCommand';
import { safeQuerySelector } from '../../../../../shared/GlobalUtils';

export class ScrollBrowserCommand extends ScrollCommand {
  
  /**
   * Browser does ONE thing: scroll to position or element
   */
  async execute(params: ScrollParams): Promise<ScrollResult> {
    console.log(`📜 BROWSER: Starting scroll operation`);

    try {
      let finalX = 0;
      let finalY = 0;

      if (params.selector) {
        // Scroll to element
        console.log(`📍 BROWSER: Scrolling to element: ${params.selector}`);
        const element = safeQuerySelector(params.selector);
        if (!element) {
          throw new Error(`Element not found: ${params.selector}`);
        }
        
        element.scrollIntoView({ 
          behavior: params.behavior,
          block: 'center',
          inline: 'nearest'
        });
        
        // Get final scroll position after scrolling
        finalX = window.scrollX;
        finalY = window.scrollY;
        
      } else {
        // Scroll to specific coordinates
        console.log(`📍 BROWSER: Scrolling to position (${params.x}, ${params.y})`);
        window.scrollTo({
          left: params.x,
          top: params.y,
          behavior: params.behavior
        });
        
        finalX = params.x || 0;
        finalY = params.y || 0;
      }

      // Wait a moment for smooth scrolling to complete
      if (params.behavior === 'smooth') {
        await new Promise(resolve => setTimeout(resolve, 500));
      }
      
      console.log(`✅ BROWSER: Scrolled to (${finalX}, ${finalY})`);
      
      return createScrollResult(params.context, params.sessionId, {
        success: true,
        scrollX: finalX,
        scrollY: finalY,
        selector: params.selector,
        scrolled: true
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Scroll failed:`, error.message);
      const scrollError = error instanceof Error ? new ValidationError('scroll', error.message, { cause: error }) : new ValidationError('scroll', String(error));
      return createScrollResult(params.context, params.sessionId, {
        success: false,
        scrollX: window.scrollX || 0,
        scrollY: window.scrollY || 0,
        selector: params.selector,
        scrolled: false,
        error: scrollError
      });
    }
  }
}