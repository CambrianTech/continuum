/**
 * HTML Inspector - Debug command to traverse shadow DOM and extract widget contents
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { HtmlInspectorParams, HtmlInspectorResult } from '../shared/HtmlInspectorTypes';
import { createHtmlInspectorResult } from '../shared/HtmlInspectorTypes';

export class HtmlInspectorBrowserCommand extends CommandBase<HtmlInspectorParams, HtmlInspectorResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/debug/html-inspector', context, subpath, commander);
  }

  async execute(params: HtmlInspectorParams): Promise<HtmlInspectorResult> {
    const { selector, includeStyles = false, maxDepth = 5 } = params;
    
    try {
      // Find element in main DOM or shadow DOM
      const element = this.findElementInShadowDOM(selector);
      
      if (!element) {
        return createHtmlInspectorResult(params.context, params.sessionId, {
          success: false,
          error: `Element not found: ${selector}`
        });
      }

      // Extract complete HTML structure including shadow DOM
      const structure = this.extractElementStructure(element, maxDepth);
      const html = this.getElementHTML(element, includeStyles);
      const text = element.textContent || '';

      // Get computed styles and dimensions
      const rect = element.getBoundingClientRect();
      const computed = window.getComputedStyle(element);

      const dimensions = {
        width: rect.width,
        height: rect.height,
        top: rect.top,
        left: rect.left,
        bottom: rect.bottom,
        right: rect.right
      };

      const computedStyles = {
        display: computed.display,
        position: computed.position,
        flexDirection: computed.flexDirection,
        height: computed.height,
        minHeight: computed.minHeight,
        maxHeight: computed.maxHeight,
        overflow: computed.overflow,
        overflowY: computed.overflowY,
        flex: computed.flex,
        flexGrow: computed.flexGrow,
        flexShrink: computed.flexShrink,
        flexBasis: computed.flexBasis
      };

      return createHtmlInspectorResult(params.context, params.sessionId, {
        success: true,
        html,
        text,
        structure,
        tagName: element.tagName,
        className: element.className,
        id: element.id,
        dimensions,
        computedStyles
      });

    } catch (error) {
      return createHtmlInspectorResult(params.context, params.sessionId, {
        success: false,
        error: `HTML inspection failed: ${(error as Error).message}`
      });
    }
  }

  private findElementInShadowDOM(selector: string): Element | null {
    // Try main document first
    let element = document.querySelector(selector);
    if (element) return element;

    // Search through all shadow DOMs
    const allElements = document.querySelectorAll('*');
    for (const el of Array.from(allElements)) {
      if (el.shadowRoot) {
        element = el.shadowRoot.querySelector(selector);
        if (element) return element;
        
        // Recursively search nested shadow DOMs
        element = this.searchNestedShadowDOM(el.shadowRoot, selector);
        if (element) return element;
      }
    }

    return null;
  }

  private searchNestedShadowDOM(root: ShadowRoot, selector: string): Element | null {
    const elements = root.querySelectorAll('*');
    for (const el of Array.from(elements)) {
      if (el.shadowRoot) {
        const found = el.shadowRoot.querySelector(selector);
        if (found) return found;
        
        const nested = this.searchNestedShadowDOM(el.shadowRoot, selector);
        if (nested) return nested;
      }
    }
    return null;
  }

  private extractElementStructure(element: Element, maxDepth: number, currentDepth: number = 0): any {
    if (currentDepth >= maxDepth) {
      return { truncated: true, reason: 'Max depth reached' };
    }

    const structure: any = {
      tagName: element.tagName,
      className: element.className,
      id: element.id,
      textContent: element.textContent?.slice(0, 100),
      attributes: {}
    };

    // Get attributes
    for (const attr of Array.from(element.attributes)) {
      structure.attributes[attr.name] = attr.value;
    }

    // Check for shadow DOM
    if (element.shadowRoot) {
      structure.shadowRoot = {
        innerHTML: element.shadowRoot.innerHTML.slice(0, 1000),
        children: Array.from(element.shadowRoot.children).map(child => 
          this.extractElementStructure(child, maxDepth, currentDepth + 1)
        )
      };
    }

    // Regular children
    if (element.children.length > 0) {
      structure.children = Array.from(element.children).slice(0, 10).map(child =>
        this.extractElementStructure(child, maxDepth, currentDepth + 1)
      );
    }

    return structure;
  }

  private getElementHTML(element: Element, includeStyles: boolean): string {
    let html = element.outerHTML;

    // If element has shadow DOM, include that too
    if (element.shadowRoot) {
      const shadowHTML = element.shadowRoot.innerHTML;
      html += `\n<!-- SHADOW DOM CONTENT -->\n${shadowHTML}`;
    }

    return html;
  }
}