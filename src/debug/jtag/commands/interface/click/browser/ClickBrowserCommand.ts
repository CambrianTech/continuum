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
import { ValidationError } from '@system/core/types/ErrorTypes';
import { ClickCommand } from '../shared/ClickCommand';
import { safeQuerySelector } from '@daemons/command-daemon/shared/GlobalUtils';

export class ClickBrowserCommand extends ClickCommand {
  
  /**
   * Browser does ONE thing: click element
   * Handles both regular selectors and widget selectors (with shadow DOM traversal)
   */
  async execute(params: ClickParams): Promise<ClickResult> {
    console.log(`👆 BROWSER: Clicking ${params.selector}${params.text ? ` (text: "${params.text}")` : ''}`);

    try {
      let element: Element | null = null;
      let clickTarget: Element | null = null;

      // Check if selector looks like a widget (ends with -widget)
      if (params.selector.endsWith('-widget')) {
        console.log(`🔍 BROWSER: Widget selector detected`);

        // Handle root and container widgets directly (not found by WidgetDiscovery)
        const widgetElement = this.findWidgetElement(params.selector);
        if (!widgetElement) {
          throw new Error(`Widget not found: ${params.selector}`);
        }

        element = widgetElement;
        const shadowRoot = (widgetElement as HTMLElement).shadowRoot;

        // If text parameter provided, find element containing that text in shadow DOM
        if (params.text && shadowRoot) {
          clickTarget = this.findElementByText(shadowRoot, params.text);
          if (!clickTarget) {
            throw new Error(`Element with text "${params.text}" not found inside ${params.selector}`);
          }
          console.log(`🎯 BROWSER: Found element with text "${params.text}" inside widget`);
        }
        // If shadowRoot and innerSelector provided, find element inside widget's shadow DOM
        else if (params.shadowRoot && params.innerSelector && shadowRoot) {
          clickTarget = shadowRoot.querySelector(params.innerSelector);
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

        // If text parameter provided, find child element with that text
        if (params.text) {
          clickTarget = this.findElementByText(element, params.text);
          if (!clickTarget) {
            throw new Error(`Element with text "${params.text}" not found inside ${params.selector}`);
          }
        } else {
          clickTarget = element;
        }
      }

      // Click the target element - use native click() for better compatibility
      console.log(`🎯 BROWSER: Clicking element:`, clickTarget.tagName, (clickTarget as HTMLElement).className);
      (clickTarget as HTMLElement).click();

      console.log(`✅ BROWSER: Clicked ${params.selector}${params.text ? ` (text: "${params.text}")` : params.innerSelector ? ` -> ${params.innerSelector}` : ''}`);

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

  /**
   * Find a widget element by selector, handling the DOM hierarchy:
   * - continuum-widget: Root element, found directly in document
   * - Container widgets (main-widget, sidebar-widget): Found in continuum-widget's shadow root
   * - Nested widgets (chat-widget, etc.): Found inside container shadow roots
   */
  private findWidgetElement(selector: string): HTMLElement | null {
    // 1. Root widget - continuum-widget
    if (selector === 'continuum-widget') {
      return document.querySelector('continuum-widget') as HTMLElement;
    }

    const continuum = document.querySelector('continuum-widget');
    if (!continuum?.shadowRoot) return null;

    // 2. Container widgets - direct children of continuum-widget
    const containerWidgets = ['main-widget', 'sidebar-widget', 'theme-widget', 'right-panel-widget'];
    if (containerWidgets.includes(selector)) {
      return continuum.shadowRoot.querySelector(selector) as HTMLElement;
    }

    // 3. Nested widgets - search inside container widgets
    for (const containerName of containerWidgets) {
      const container = continuum.shadowRoot.querySelector(containerName) as HTMLElement;
      if (container?.shadowRoot) {
        const widget = container.shadowRoot.querySelector(selector) as HTMLElement;
        if (widget) {
          console.log(`🔍 BROWSER: Found ${selector} inside ${containerName}`);
          return widget;
        }
      }
    }

    return null;
  }

  /**
   * Find a clickable element containing the specified text
   * Returns the clickable parent (like .content-tab) rather than inner text spans
   *
   * CRITICAL: This recursively searches nested shadow DOMs to find elements in
   * deeply nested widget structures like:
   * main-widget (shadow) → content-tabs-widget (shadow) → .content-tab
   */
  private findElementByText(root: Element | ShadowRoot, text: string): Element | null {
    // First, search direct children in this root
    const result = this.searchInRoot(root, text);
    if (result) return result;

    // Then recursively search nested shadow DOMs
    const allElements = root.querySelectorAll('*');
    for (const el of Array.from(allElements)) {
      const shadowRoot = (el as HTMLElement).shadowRoot;
      if (shadowRoot) {
        console.log(`🔍 BROWSER: Searching nested shadow DOM in ${el.tagName.toLowerCase()}`);
        const nestedResult = this.findElementByText(shadowRoot, text);
        if (nestedResult) return nestedResult;
      }
    }

    return null;
  }

  /**
   * Search within a single root (shadow root or element) for text match
   */
  private searchInRoot(root: Element | ShadowRoot, text: string): Element | null {
    // Get all elements that could be clickable
    const candidates = Array.from(root.querySelectorAll('*'));

    for (const el of candidates) {
      // Skip elements that have shadow roots - we'll search those recursively
      if ((el as HTMLElement).shadowRoot) continue;

      // Check if this element's direct text content matches
      const childNodes = Array.from(el.childNodes);
      const directText = childNodes
        .filter((node: ChildNode) => node.nodeType === Node.TEXT_NODE)
        .map((node: ChildNode) => (node as Text).textContent?.trim() || '')
        .join('');

      if (directText === text || el.textContent?.trim() === text) {
        // If this is a span/label inside a clickable container, return the container
        const parent = el.parentElement;
        if (parent) {
          // Check if parent is the actual clickable element (has data-tab-id, is a button, etc)
          if ((parent as HTMLElement).dataset?.tabId ||
              parent.classList.contains('content-tab') ||
              parent.classList.contains('room-item') ||
              parent.tagName.toLowerCase() === 'button') {
            console.log(`🎯 BROWSER: Found text "${text}" in child, returning parent:`, parent.tagName, parent.className);
            return parent;
          }
        }

        // Prefer elements that look clickable
        const tagName = el.tagName.toLowerCase();
        if (['button', 'a', 'div', 'li'].includes(tagName) ||
            el.classList.contains('tab') ||
            el.classList.contains('content-tab') ||
            el.classList.contains('room-item') ||
            (el as HTMLElement).dataset?.tabId ||
            (el as HTMLElement).onclick !== null) {
          return el;
        }
      }
    }

    // Second pass: find any element containing the text
    for (const el of candidates) {
      if ((el as HTMLElement).shadowRoot) continue;  // Skip shadow hosts

      if (el.textContent?.trim() === text) {
        // Again, prefer parent if it's clickable
        const parent = el.parentElement;
        if (parent && ((parent as HTMLElement).dataset?.tabId || parent.classList.contains('content-tab'))) {
          return parent;
        }
        return el;
      }
    }

    return null;
  }
}