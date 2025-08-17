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
 * FIXED: Accounts for scroll offsets and viewport positioning
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
  
  // Get element dimensions from getBoundingClientRect for size
  const elementRect = element.getBoundingClientRect();
  
  // CRITICAL FIX: Use offsetTop/offsetLeft for document-relative positioning
  // getBoundingClientRect gives viewport position, but html2canvas captures full document
  const htmlElement = element as HTMLElement;
  const documentX = htmlElement.offsetLeft || 0;
  const documentY = htmlElement.offsetTop || 0;
  
  // Use scroll dimensions for overflow content
  const elementScrollWidth = htmlElement.scrollWidth || elementRect.width;
  const elementScrollHeight = htmlElement.scrollHeight || elementRect.height;
  
  // Determine actual element dimensions (visual bounds vs full content)
  const actualWidth = includeOverflow ? Math.max(elementRect.width, elementScrollWidth) : elementRect.width;
  const actualHeight = includeOverflow ? Math.max(elementRect.height, elementScrollHeight) : elementRect.height;
  
  // Use document-relative coordinates for accurate canvas positioning
  const relativeX = Math.max(0, documentX * scale);
  const relativeY = Math.max(0, documentY * scale);
  const relativeWidth = actualWidth * scale;
  const relativeHeight = actualHeight * scale;
  
  console.log(`📐 ElementUtils: Element ${getElementDisplayName(element)} - viewport: ${elementRect.left},${elementRect.top}, document: ${documentX},${documentY}, size: ${actualWidth}x${actualHeight}`);
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
  const constrainedWidth = Math.min(crop.width, canvasWidth - crop.x);
  const constrainedHeight = Math.min(crop.height, canvasHeight - crop.y);
  
  if (constrainedWidth !== crop.width || constrainedHeight !== crop.height) {
    console.log(`⚠️ ElementUtils: Constrained crop from ${crop.width}x${crop.height} to ${constrainedWidth}x${constrainedHeight} to fit canvas`);
  }
  
  return {
    ...crop,
    width: constrainedWidth,
    height: constrainedHeight
  };
}

/**
 * Get element name for debugging (ID > class > tag)
 */
export function getElementDisplayName(element: Element): string {
  if (element.id) return `#${element.id}`;
  if (element.classList.length > 0) return `.${element.classList[0]}`;
  return element.tagName.toLowerCase();
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