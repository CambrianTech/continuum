/**
 * SHADOW DOM UTILITIES
 * 
 * Universal utilities for traversing Shadow DOM in browser environments.
 * These utilities are integrated into commands like get-text, screenshot, etc.
 * to handle Shadow DOM widgets seamlessly.
 * 
 * PURE TYPESCRIPT - No eval() or inline code generation
 */

export interface ShadowDOMSearchResult {
  found: boolean;
  elements: ShadowDOMElement[];
  mainDocumentMatches?: any[];
  totalShadowRoots: number;
  searchPath: string;
  options?: ShadowDOMQueryOptions;
}

export interface ShadowDOMElement {
  hostTag: string;
  hostId?: string;
  hostClass?: string;
  shadowContent?: string;
  matchingElements: {
    tag: string;
    id?: string;
    class?: string;
    text?: string;
    innerHTML?: string;
    attributes?: Record<string, string>;
  }[];
}

export interface ShadowDOMQueryOptions {
  /**
   * Standard CSS selector - will be applied inside Shadow DOM
   */
  querySelector?: string;
  
  /**
   * Text content to search for
   */
  textContent?: string;
  
  /**
   * Attribute to search for
   */
  attribute?: { name: string; value?: string };
  
  /**
   * Whether to include shadow DOM content in results
   */
  includeShadowContent?: boolean;
  
  /**
   * Maximum depth to traverse Shadow DOM trees
   */
  maxDepth?: number;
  
  /**
   * Whether to search in main document as well
   */
  includeMainDocument?: boolean;
}

/**
 * SHADOW DOM QUERY BUILDER - TYPE-SAFE APPROACH
 * 
 * Creates query options for Shadow DOM traversal without eval() or string injection.
 * Used by browser-side implementations for actual DOM manipulation.
 */
export function createShadowDOMQueryOptions(selector: string, options: Partial<ShadowDOMQueryOptions> = {}): ShadowDOMQueryOptions {
  return {
    querySelector: selector,
    includeMainDocument: options.includeMainDocument ?? true,
    includeShadowContent: options.includeShadowContent ?? false,
    maxDepth: options.maxDepth ?? 10,
    ...options
  };
}

/**
 * ENHANCED SELECTOR - Combines standard and Shadow DOM selectors
 * 
 * Usage: "button" -> searches both main document and Shadow DOM
 * Usage: "shadow:button" -> searches only Shadow DOM
 * Usage: "main:button" -> searches only main document
 */
export function parseEnhancedSelector(selector: string): {
  actualSelector: string;
  searchTarget: 'both' | 'shadow' | 'main';
  queryOptions: ShadowDOMQueryOptions;
} {
  let searchTarget: 'both' | 'shadow' | 'main' = 'both';
  let actualSelector = selector;
  
  if (selector.startsWith('shadow:')) {
    searchTarget = 'shadow';
    actualSelector = selector.substring(7);
  } else if (selector.startsWith('main:')) {
    searchTarget = 'main';
    actualSelector = selector.substring(5);
  }
  
  const queryOptions: ShadowDOMQueryOptions = {
    querySelector: actualSelector,
    includeMainDocument: searchTarget === 'both' || searchTarget === 'main',
    includeShadowContent: true,
    maxDepth: 10
  };
  
  return {
    actualSelector,
    searchTarget,
    queryOptions
  };
}