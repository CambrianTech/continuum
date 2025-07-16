/**
 * Client-side screenshot capture functionality
 * Handles html2canvas integration and file saving
 * 
 * MIGRATION NOTE: This file now delegates to the new unified ScreenshotClient
 * in the client/ directory following the middle-out architecture pattern.
 */

import type { ScreenshotClientRequest, ScreenshotResult } from './ScreenshotTypes';
import { clientScreenshot as newClientScreenshot } from './client/ScreenshotClient';

interface ScreenshotClientParams extends ScreenshotClientRequest {
  // Directory will be determined by session context, not passed explicitly
}

/**
 * Capture screenshot using html2canvas with AI-friendly features
 * Supports element targeting, scaling, cropping, and compression
 * 
 * MIGRATION: This function now delegates to the new unified ScreenshotClient
 */
export async function clientScreenshot(params: ScreenshotClientParams): Promise<ScreenshotResult> {
  return await newClientScreenshot(params);
}

// Make function available globally for eval execution
(window as any).clientScreenshot = clientScreenshot;