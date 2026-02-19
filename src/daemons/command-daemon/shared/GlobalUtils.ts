/**
 * Global Utilities - Cross-platform global context helpers
 * 
 * Centralized utilities for safely accessing global context and APIs
 * with proper TypeScript typing across browser, server, and other environments.
 */

/**
 * Universal Global Context Interface
 * Works in browser (window), Node.js (global), and other JavaScript environments
 */
export interface GlobalContext {
  // Universal properties (available in most environments)
  console?: Console;
  setTimeout?: typeof setTimeout;
  clearTimeout?: typeof clearTimeout;
  
  // Browser-specific properties (when available)
  document?: Document;
  window?: Window;
  innerWidth?: number;
  innerHeight?: number;
  location?: Location;
  navigator?: Navigator;
  localStorage?: Storage;
  sessionStorage?: Storage;
  
  // Node.js-specific properties (when available)
  process?: unknown;
  Buffer?: unknown;
  require?: unknown;
  
  // Allow for any additional APIs
  [key: string]: unknown;
}

/**
 * Get properly typed global context
 * Eliminates need for `as any` casts throughout the codebase
 * Works in browser (window), Node.js (global), and other environments
 */
export function getGlobalContext(): GlobalContext {
  // Use globalThis as the universal approach
  return globalThis as GlobalContext;
}

/**
 * Check if we're running in a browser environment
 */
export function isBrowserEnvironment(): boolean {
  const global = globalThis as GlobalContext;
  return typeof global.window !== 'undefined' && typeof global.document !== 'undefined';
}

/**
 * Get a specific global API with type safety
 * Returns undefined if the API is not available
 * Works across browser, Node.js, and other environments
 */
export function getGlobalAPI<T = unknown>(apiName: string): T | undefined {
  const context = getGlobalContext();
  return context[apiName] as T | undefined;
}

/**
 * Check if we're running in a Node.js/server environment
 */
export function isServerEnvironment(): boolean {
  return typeof process !== 'undefined' && Boolean(process?.versions?.node);
}

/**
 * Check if we're running in a Web Worker environment
 */
export function isWebWorkerEnvironment(): boolean {
  const globalScope = getGlobalContext();
  return typeof globalScope.importScripts === 'function' && 
         typeof globalScope.WorkerGlobalScope !== 'undefined';
}

/**
 * Get environment type
 */
export function getEnvironmentType(): 'browser' | 'server' | 'webworker' | 'unknown' {
  if (isBrowserEnvironment()) return 'browser';
  if (isServerEnvironment()) return 'server';
  if (isWebWorkerEnvironment()) return 'webworker';
  return 'unknown';
}

/**
 * Safely access DOM elements with proper error handling
 * Automatically pierces shadow DOM to find elements
 * Only works in browser environment - returns null otherwise
 */
export function safeQuerySelector(selector: string): Element | null {
  if (!isBrowserEnvironment()) return null;

  const context = getGlobalContext();
  try {
    // First try regular document query
    const directMatch = context.document?.querySelector(selector);
    if (directMatch) return directMatch;

    // If not found, search through all shadow DOMs
    return querySelectorDeep(selector, context.document?.body ?? null);
  } catch (error) {
    console.warn(`Failed to query selector "${selector}":`, error);
    return null;
  }
}

/**
 * Recursively search through shadow DOMs for an element
 * Automatically pierces all shadow roots to find the selector
 */
export function querySelectorDeep(selector: string, root: Element | null): Element | null {
  if (!root) return null;

  // Try to find in this element's shadow root
  const shadowRoot = (root as any).shadowRoot as ShadowRoot | null;
  if (shadowRoot) {
    const match = shadowRoot.querySelector(selector);
    if (match) return match;

    // Search children within shadow root
    const shadowChildren = shadowRoot.querySelectorAll('*');
    for (const child of Array.from(shadowChildren)) {
      const found = querySelectorDeep(selector, child);
      if (found) return found;
    }
  }

  // Search children in light DOM
  const children = root.querySelectorAll('*');
  for (const child of Array.from(children)) {
    // Check if child has shadow root
    const childShadow = (child as any).shadowRoot as ShadowRoot | null;
    if (childShadow) {
      const match = childShadow.querySelector(selector);
      if (match) return match;

      // Recurse into shadow root's children
      const found = querySelectorDeep(selector, child);
      if (found) return found;
    }
  }

  return null;
}

/**
 * Get viewport dimensions with fallbacks
 * Only works in browser environment - returns defaults otherwise
 */
export function getViewportDimensions(): { width: number; height: number } {
  if (!isBrowserEnvironment()) {
    return { width: 800, height: 600 };
  }
  
  const context = getGlobalContext();
  return {
    width: context.innerWidth ?? 800,
    height: context.innerHeight ?? 600
  };
}