/**
 * Browser Element Utilities - Modular DOM measurement and coordinate functions
 * 
 * Centralized utilities for accurate element measurement, coordinate calculation,
 * and content boundary detection across different element types and layouts.
 */

import { isBrowserEnvironment } from '../../../../daemons/command-daemon/shared/GlobalUtils';

export interface ElementBounds {
  x: number;
  y: number;
  width: number;
  height: number;
  scrollWidth?: number;
  scrollHeight?: number;
  hasOverflow: boolean;
}

export interface CropCoordinates {
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

// Additional interfaces for modular functions
export interface ViewportCoordinates {
  left: number;
  top: number;
  width: number;
  height: number;
}

export interface ScrollOffset {
  x: number;
  y: number;
}

export interface DocumentCoordinates {
  x: number;
  y: number;
}

/**
 * Get accurate element bounds including overflow content
 * BREAKTHROUGH: Accounts for content that extends beyond getBoundingClientRect()
 */
export function getElementBounds(element: Element, includeOverflow: boolean = true): ElementBounds {
  if (!isBrowserEnvironment()) {
    throw new Error('getElementBounds only available in browser environment');
  }
  
  const rect = element.getBoundingClientRect();
  const computedStyle = window.getComputedStyle(element);
  
  // For chat widget and similar components, use visual bounds as primary source
  // Only fallback to scroll dimensions if they're meaningfully larger
  const scrollWidth = (element as HTMLElement).scrollWidth || rect.width;
  const scrollHeight = (element as HTMLElement).scrollHeight || rect.height;
  
  // More conservative overflow detection - only use scroll dimensions if significantly larger
  const scrollThreshold = 10; // pixels
  const hasScrollOverflow = (scrollWidth > rect.width + scrollThreshold) || 
                            (scrollHeight > rect.height + scrollThreshold);
  
  // For widgets, prioritize visual bounds unless there's clear scroll overflow
  const effectiveWidth = includeOverflow && hasScrollOverflow ? scrollWidth : rect.width;
  const effectiveHeight = includeOverflow && hasScrollOverflow ? scrollHeight : rect.height;
  
  console.log(`📐 ElementUtils: Element bounds - rect: ${rect.width}x${rect.height}, scroll: ${scrollWidth}x${scrollHeight}, effective: ${effectiveWidth}x${effectiveHeight}`);
  
  return {
    x: rect.left,
    y: rect.top,
    width: effectiveWidth,
    height: effectiveHeight,
    scrollWidth,
    scrollHeight,
    hasOverflow: hasScrollOverflow
  };
}

// =============================================================================
// MODULAR COORDINATE CALCULATION FUNCTIONS
// =============================================================================

/**
 * Get current page scroll offset
 * Pure function - testable in isolation
 */
export function getPageScrollOffset(): { x: number; y: number } {
  return {
    x: window.pageXOffset || document.documentElement.scrollLeft || 0,
    y: window.pageYOffset || document.documentElement.scrollTop || 0
  };
}

/**
 * Get element viewport coordinates
 * Pure function - returns getBoundingClientRect data
 */
export function getViewportCoordinates(element: Element): { left: number; top: number; width: number; height: number } {
  const rect = element.getBoundingClientRect();
  return {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height
  };
}

/**
 * Convert viewport coordinates to document coordinates
 * Pure function - testable with mock data
 */
export function viewportToDocumentCoords(
  viewportCoords: { left: number; top: number },
  scrollOffset: { x: number; y: number }
): { x: number; y: number } {
  return {
    x: viewportCoords.left + scrollOffset.x,
    y: viewportCoords.top + scrollOffset.y
  };
}

/**
 * Apply scaling to coordinates and dimensions
 * Pure function - easily testable
 */
export function applyCoordinateScaling(
  coords: { x: number; y: number; width: number; height: number },
  scale: number
): { x: number; y: number; width: number; height: number } {
  return {
    x: Math.round(coords.x * scale),
    y: Math.round(coords.y * scale),
    width: Math.round(coords.width * scale),
    height: Math.round(coords.height * scale)
  };
}

/**
 * Get absolute position of element in document coordinates
 * Composed from smaller, testable functions
 */
export function getAbsolutePosition(element: Element): { x: number; y: number } {
  const viewportCoords = getViewportCoordinates(element);
  const scrollOffset = getPageScrollOffset();
  return viewportToDocumentCoords(viewportCoords, scrollOffset);
}

/**
 * Calculate crop coordinates for screenshot
 * 🔧 CLAUDE-FIX-2024-08-27-D: Modular, testable coordinate calculation
 */
export function calculateCropCoordinates(
  element: Element, 
  bodyElement: Element = document.body,
  scale: number = 1,
  includeOverflow: boolean = true
): CropCoordinates {
  if (!isBrowserEnvironment()) {
    throw new Error('calculateCropCoordinates only available in browser environment');
  }
  
  console.log('🔧 CLAUDE-FIX-2024-08-27-D: Modular, testable coordinate calculation');
  
  // Step 1: Get viewport coordinates
  const viewportCoords = getViewportCoordinates(element);
  console.log(`📐 ElementUtils: Viewport: ${viewportCoords.left}, ${viewportCoords.top}, ${viewportCoords.width}x${viewportCoords.height}`);
  
  // Step 2: Get scroll offset
  const scrollOffset = getPageScrollOffset();
  console.log(`📐 ElementUtils: Scroll: ${scrollOffset.x}, ${scrollOffset.y}`);
  
  // Step 3: Convert to document coordinates
  const documentCoords = viewportToDocumentCoords(viewportCoords, scrollOffset);
  console.log(`📐 ElementUtils: Document: ${documentCoords.x}, ${documentCoords.y}`);
  
  // Step 4: Combine coordinates and dimensions
  const fullCoords = {
    x: documentCoords.x,
    y: documentCoords.y,
    width: viewportCoords.width,
    height: viewportCoords.height
  };
  
  // Step 5: Apply scaling
  const scaledCoords = applyCoordinateScaling(fullCoords, scale);
  console.log(`📐 ElementUtils: Final crop: ${scaledCoords.x}, ${scaledCoords.y}, ${scaledCoords.width}x${scaledCoords.height}`);
  
  return {
    ...scaledCoords,
    scale
  };
}

/**
 * Constrain crop coordinates to canvas bounds (safety check)
 */
export function constrainCropToCanvas(
  crop: CropCoordinates,
  canvasWidth: number,
  canvasHeight: number
): CropCoordinates {
  // Calculate the actual visible area when crop extends outside canvas
  const cropEndX = crop.x + crop.width;
  const cropEndY = crop.y + crop.height;
  
  // Clamp start coordinates to canvas bounds
  const constrainedStartX = Math.max(0, crop.x);
  const constrainedStartY = Math.max(0, crop.y);
  
  // Clamp end coordinates to canvas bounds  
  const constrainedEndX = Math.min(canvasWidth, cropEndX);
  const constrainedEndY = Math.min(canvasHeight, cropEndY);
  
  // Calculate dimensions of visible area
  const constrainedWidth = Math.max(1, constrainedEndX - constrainedStartX);
  const constrainedHeight = Math.max(1, constrainedEndY - constrainedStartY);
  
  if (constrainedStartX !== crop.x || constrainedStartY !== crop.y || 
      constrainedWidth !== crop.width || constrainedHeight !== crop.height) {
    console.log(`⚠️ ElementUtils: Constrained crop from (${crop.x},${crop.y},${crop.width}x${crop.height}) to (${constrainedStartX},${constrainedStartY},${constrainedWidth}x${constrainedHeight}) to fit canvas`);
  }
  
  return {
    x: constrainedStartX,
    y: constrainedStartY,
    width: constrainedWidth,
    height: constrainedHeight,
    scale: crop.scale
  };
}

/**
 * Get element name for debugging (ID > class > tag)
 */
export function getElementDisplayName(element: Element): string {
  if (element.id) return `#${element.id}`;
  if (element.classList && element.classList.length > 0) return `.${element.classList[0]}`;
  return element.tagName?.toLowerCase() || 'unknown';
}

/**
 * Shadow DOM-aware recursive element finder
 * Traverses through shadow DOMs to find web components like chat-widget
 */
function findInShadowDOM(root: Document | DocumentFragment | Element, selector: string): Element | null {
  // Check current root with standard querySelector
  const direct = root.querySelector(selector);
  if (direct) return direct;
  
  // Recursively check all shadow roots
  const elements = root.querySelectorAll('*');
  for (const el of elements) {
    if (el.shadowRoot) {
      const found = findInShadowDOM(el.shadowRoot, selector);
      if (found) return found;
    }
  }
  return null;
}

/**
 * Enhanced element selector with shadow DOM traversal and content boundary detection
 * BREAKTHROUGH: Finds web components inside shadow DOMs
 */
export function smartQuerySelector(selector: string): Element | null {
  if (!isBrowserEnvironment()) return null;
  
  try {
    // First try standard querySelector (fastest for regular elements)
    let element = document.querySelector(selector);
    
    // If not found, try shadow DOM traversal (for web components)
    if (!element) {
      console.log(`🔍 ElementUtils: Standard querySelector failed for "${selector}", trying shadow DOM traversal...`);
      element = findInShadowDOM(document, selector);
    }
    
    if (!element) {
      console.warn(`❌ ElementUtils: Element not found: "${selector}" (checked regular DOM and shadow DOMs)`);
      return null;
    }
    
    // Log element info for debugging
    const bounds = getElementBounds(element);
    const displayName = getElementDisplayName(element);
    const inShadowDOM = element.getRootNode() !== document;
    
    console.log(`🎯 ElementUtils: Found ${displayName}${inShadowDOM ? ' (in shadow DOM)' : ''} - bounds: ${bounds.width}x${bounds.height}${bounds.hasOverflow ? ' (has overflow)' : ''}`);
    
    return element;
  } catch (error) {
    console.warn(`❌ ElementUtils: Failed to query selector "${selector}":`, error);
    return null;
  }
}