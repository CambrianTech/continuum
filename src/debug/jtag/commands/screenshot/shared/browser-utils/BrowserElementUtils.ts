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

/**
 * Calculate crop coordinates relative to body with proper content detection
 * 🔧 CLAUDE-FIX-2024-08-27-A: Fixed coordinate calculation to prevent cropping bugs
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
  
  console.log('🔧 CLAUDE-FIX-2024-08-27-A: Enhanced coordinate calculation active');
  
  // Get element and body dimensions
  const elementRect = element.getBoundingClientRect();
  const bodyRect = bodyElement.getBoundingClientRect();
  
  // CRITICAL FIX: Account for scroll position in coordinate calculation
  const scrollX = window.pageXOffset || document.documentElement.scrollLeft || 0;
  const scrollY = window.pageYOffset || document.documentElement.scrollTop || 0;
  
  console.log(`📐 ElementUtils: Scroll position: ${scrollX}, ${scrollY}`);
  console.log(`📐 ElementUtils: Element rect: ${elementRect.left}, ${elementRect.top}, ${elementRect.width}x${elementRect.height}`);
  console.log(`📐 ElementUtils: Body rect: ${bodyRect.left}, ${bodyRect.top}`);
  
  // Calculate element position relative to body INCLUDING scroll offset
  const relativeX = Math.max(0, ((elementRect.left + scrollX) - (bodyRect.left + scrollX)) * scale);
  const relativeY = Math.max(0, ((elementRect.top + scrollY) - (bodyRect.top + scrollY)) * scale);
  
  // Get element dimensions with overflow consideration
  const elementBounds = getElementBounds(element, includeOverflow);
  
  // CRITICAL FIX: Use actual element rect dimensions instead of elementBounds for more reliable cropping
  const actualWidth = elementRect.width;
  const actualHeight = elementRect.height;
  
  const relativeWidth = actualWidth * scale;
  const relativeHeight = actualHeight * scale;
  
  console.log(`🔧 CLAUDE-FIX-2024-08-27-A: Using actual rect dimensions ${actualWidth}x${actualHeight} instead of bounds ${elementBounds.width}x${elementBounds.height}`);
  console.log(`📐 ElementUtils: Element ${getElementDisplayName(element)} - viewport: ${elementRect.left},${elementRect.top}, relative to body: ${relativeX/scale},${relativeY/scale}, size: ${actualWidth}x${actualHeight}`);
  console.log(`📐 ElementUtils: Final crop: ${relativeX},${relativeY} ${relativeWidth}x${relativeHeight} @ scale ${scale}`);
  
  return {
    x: Math.round(relativeX),
    y: Math.round(relativeY), 
    width: Math.round(relativeWidth),
    height: Math.round(relativeHeight),
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
 * Enhanced element selector with content boundary detection
 */
export function smartQuerySelector(selector: string): Element | null {
  if (!isBrowserEnvironment()) return null;
  
  try {
    const element = document.querySelector(selector);
    if (!element) return null;
    
    // Log element info for debugging
    const bounds = getElementBounds(element);
    const displayName = getElementDisplayName(element);
    
    console.log(`🎯 ElementUtils: Found ${displayName} - bounds: ${bounds.width}x${bounds.height}${bounds.hasOverflow ? ' (has overflow)' : ''}`);
    
    return element;
  } catch (error) {
    console.warn(`❌ ElementUtils: Failed to query selector "${selector}":`, error);
    return null;
  }
}