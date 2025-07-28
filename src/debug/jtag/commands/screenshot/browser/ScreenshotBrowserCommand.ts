/**
 * Screenshot Command - Browser Implementation (Simplified)
 * 
 * MINIMAL WORK PER COMMAND: Just implements what browser does
 */

import type { ScreenshotParams, Html2CanvasCanvas, Html2CanvasOptions, ScreenshotResult } from '@screenshotShared/ScreenshotTypes';
import { createScreenshotResultFromParams, createScreenshotResult } from '@screenshotShared/ScreenshotTypes';
import { ScreenshotCommand } from '@screenshotShared/ScreenshotCommand';
import { getGlobalAPI, safeQuerySelector, getViewportDimensions } from '@shared/GlobalUtils';

const DEFAULT_FORMAT = 'png';
const DEFAULT_QUALITY = 0.9;

export class ScreenshotBrowserCommand extends ScreenshotCommand {
  
  /**
   * Browser does ONE thing: capture screenshot with html2canvas
   * Then either returns data OR delegates to server for saving
   */
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    console.log(`📸 BROWSER: Capturing screenshot`);

    try {
      // Get html2canvas API with proper typing
      const html2canvas = getGlobalAPI<(element: Element, options?: Html2CanvasOptions) => Promise<Html2CanvasCanvas>>('html2canvas');
      if (!html2canvas) {
        throw new Error('html2canvas not available');
      }

      // Get target element safely
      const targetElement = params.selector 
        ? safeQuerySelector(params.selector)
        : safeQuerySelector('body');
        
      if (!targetElement) {
        throw new Error(`Element not found: ${params.selector ?? 'body'}`);
      }

      // Build html2canvas options with viewport dimensions
      const startTime = Date.now();
      const viewport = getViewportDimensions();
      const captureOptions: Html2CanvasOptions = {
        height: viewport.height,
        width: viewport.width,
        scrollX: 0,
        scrollY: 0,
        useCORS: true,
        ...params.options?.html2canvasOptions
      };

      // Apply specific options if provided
      if (params.options) {
        if (params.options.width) captureOptions.width = params.options.width;
        if (params.options.height) captureOptions.height = params.options.height;
        if (params.options.scale) captureOptions.scale = params.options.scale;
        if (params.options.backgroundColor) captureOptions.backgroundColor = params.options.backgroundColor;
      }

      console.log(`📷 BROWSER: Capturing ${params.selector ?? 'body'}`);
      const canvas: Html2CanvasCanvas = await html2canvas(targetElement, captureOptions);
      
      // Convert with specified format and quality
      const format = params.options?.format ?? DEFAULT_FORMAT;
      const quality = params.options?.quality ?? DEFAULT_QUALITY;
      const dataUrl = canvas.toDataURL(`image/${format}`, quality);
      
      const captureTime = Date.now() - startTime;
      console.log(`✅ BROWSER: Captured (${canvas.width}x${canvas.height}) in ${captureTime}ms`);
      
      // Enrich original params with captured data and metadata
      params.dataUrl = dataUrl;
      params.filename = params.filename ?? `screenshot-${Date.now()}.png`;
      params.metadata = {
        width: canvas.width,
        height: canvas.height,
        size: dataUrl.length,
        selector: params.selector,
        format: format,
        captureTime: captureTime
      };
      
      // Simple decision: if browser-initiated, send to server for saving
      if (!params.returnToSource) {
        console.log(`🔀 BROWSER: Sending to server for saving`);
        return await this.remoteExecute(params);
      }
      
      // Otherwise create result for calling server
      console.log(`🔙 BROWSER: Returning data to server`);
      return createScreenshotResultFromParams(params, {
        success: true,
        filepath: ''
        // filename, options, dataUrl, metadata auto-inherited from params!
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Failed:`, error.message);
      return createScreenshotResult(params.context, params.sessionId, {
        success: false,
        filepath: '',
        filename: params.filename || `screenshot-error-${Date.now()}.png`,
        error: error.message
      });
    }
  }
}