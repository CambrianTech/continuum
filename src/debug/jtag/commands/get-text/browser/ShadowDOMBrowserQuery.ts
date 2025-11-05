/**
 * Shadow DOM Browser Query - Pure TypeScript Implementation
 * 
 * Provides Shadow DOM traversal capabilities without eval() or inline code.
 * Direct browser-side implementation for real DOM manipulation.
 */

import { ShadowDOMSearchResult, ShadowDOMElement, ShadowDOMQueryOptions } from '../../../system/browser/WidgetUtils';

export class ShadowDOMBrowserQuery {
  
  /**
   * Search for elements in Shadow DOM trees
   */
  static searchShadowDOM(options: ShadowDOMQueryOptions): ShadowDOMSearchResult {
    const results: ShadowDOMElement[] = [];
    let totalShadowRoots = 0;
    
    // Get all elements in the document
    const allElements = document.querySelectorAll('*');

    // Traverse all Shadow DOM trees
    for (const element of Array.from(allElements)) {
      if (element.shadowRoot) {
        totalShadowRoots++;
        const shadowResults = this.traverseShadowRoot(element, options);
        results.push(...shadowResults);
      }
    }
    
    // Search main document if requested
    const mainDocResults = this.searchMainDocument(options);
    
    return {
      found: results.length > 0 || mainDocResults.length > 0,
      elements: results,
      mainDocumentMatches: mainDocResults,
      totalShadowRoots,
      searchPath: 'Shadow DOM + ' + (options.includeMainDocument ? 'Main Document' : 'Shadow Only'),
      options: options
    };
  }
  
  /**
   * Extract text from Shadow DOM query results
   */
  static extractTextFromResults(searchResult: ShadowDOMSearchResult): {
    found: boolean;
    text: string;
    allTexts: string[];
  } {
    const allTexts: string[] = [];
    
    // Extract text from Shadow DOM matches
    for (const element of searchResult.elements) {
      for (const match of element.matchingElements) {
        if (match.text) {
          allTexts.push(match.text);
        }
      }
    }
    
    // Extract text from main document matches
    if (searchResult.mainDocumentMatches) {
      for (const match of searchResult.mainDocumentMatches) {
        if ('textContent' in match && match.textContent) {
          allTexts.push(match.textContent);
        }
      }
    }
    
    return {
      found: allTexts.length > 0,
      text: allTexts.join('\n\n--- SEPARATOR ---\n\n'),
      allTexts
    };
  }
  
  /**
   * Recursively traverse Shadow DOM trees
   */
  private static traverseShadowRoot(element: Element, options: ShadowDOMQueryOptions, depth = 0): ShadowDOMElement[] {
    const results: ShadowDOMElement[] = [];
    const maxDepth = options.maxDepth || 10;
    
    if (depth > maxDepth || !element.shadowRoot) {
      return results;
    }
    
    const shadowRoot = element.shadowRoot;
    const hostInfo: ShadowDOMElement = {
      hostTag: element.tagName.toLowerCase(),
      hostId: element.id || undefined,
      hostClass: element.className || undefined,
      shadowContent: options.includeShadowContent ? shadowRoot.innerHTML.substring(0, 1000) : undefined,
      matchingElements: []
    };
    
    // Apply querySelector if provided
    if (options.querySelector) {
      try {
        const matches = shadowRoot.querySelectorAll(options.querySelector);
        for (const match of Array.from(matches)) {
          hostInfo.matchingElements.push({
            tag: match.tagName.toLowerCase(),
            id: match.id || undefined,
            class: match.className || undefined,
            text: match.textContent?.substring(0, 500) || undefined,
            innerHTML: match.innerHTML?.substring(0, 1000) || undefined,
            attributes: this.getElementAttributes(match)
          });
        }
      } catch (e) {
        console.warn('Shadow DOM querySelector failed:', (e as Error).message);
      }
    }
    
    // Search by text content
    if (options.textContent) {
      const allElements = shadowRoot.querySelectorAll('*');
      for (const el of Array.from(allElements)) {
        if (el.textContent && el.textContent.includes(options.textContent)) {
          hostInfo.matchingElements.push({
            tag: el.tagName.toLowerCase(),
            id: el.id || undefined,
            class: el.className || undefined,
            text: el.textContent.substring(0, 500),
            innerHTML: el.innerHTML?.substring(0, 1000) || undefined,
            attributes: this.getElementAttributes(el)
          });
        }
      }
    }
    
    // Search by attribute
    if (options.attribute) {
      const selector = options.attribute.value 
        ? `[${options.attribute.name}="${options.attribute.value}"]`
        : `[${options.attribute.name}]`;
      
      try {
        const matches = shadowRoot.querySelectorAll(selector);
        for (const match of Array.from(matches)) {
          hostInfo.matchingElements.push({
            tag: match.tagName.toLowerCase(),
            id: match.id || undefined,
            class: match.className || undefined,
            text: match.textContent?.substring(0, 500) || undefined,
            innerHTML: match.innerHTML?.substring(0, 1000) || undefined,
            attributes: this.getElementAttributes(match)
          });
        }
      } catch (e) {
        console.warn('Shadow DOM attribute search failed:', (e as Error).message);
      }
    }
    
    // Add to results if we found anything
    if (hostInfo.matchingElements.length > 0) {
      results.push(hostInfo);
    }
    
    // Recursively search nested shadow DOM
    const nestedElements = shadowRoot.querySelectorAll('*');
    for (const nested of Array.from(nestedElements)) {
      if (nested.shadowRoot) {
        results.push(...this.traverseShadowRoot(nested, options, depth + 1));
      }
    }
    
    return results;
  }
  
  /**
   * Search main document if requested
   */
  private static searchMainDocument(options: ShadowDOMQueryOptions): any[] {
    const results: any[] = [];
    
    if (!options.includeMainDocument) return results;
    
    if (options.querySelector) {
      try {
        const matches = document.querySelectorAll(options.querySelector);
        for (const match of Array.from(matches)) {
          results.push({
            tag: match.tagName.toLowerCase(),
            id: match.id || undefined,
            class: match.className || undefined,
            text: match.textContent?.substring(0, 500) || undefined,
            innerHTML: match.innerHTML?.substring(0, 1000) || undefined,
            attributes: this.getElementAttributes(match)
          });
        }
      } catch (e) {
        console.warn('Main document querySelector failed:', (e as Error).message);
      }
    }
    
    return results;
  }
  
  /**
   * Get all attributes of an element
   */
  private static getElementAttributes(element: Element): Record<string, string> {
    const attrs: Record<string, string> = {};
    for (const attr of Array.from(element.attributes)) {
      attrs[attr.name] = attr.value;
    }
    return attrs;
  }
}