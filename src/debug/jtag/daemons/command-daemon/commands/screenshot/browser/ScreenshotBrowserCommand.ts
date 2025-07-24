/**
 * Screenshot Command - Browser Implementation (Simplified)
 * 
 * MINIMAL WORK PER COMMAND: Just implements what browser does
 */

import type { ScreenshotParams } from '../shared/ScreenshotTypes';
import { ScreenshotResult } from '../shared/ScreenshotTypes';
import { ScreenshotCommand } from '../shared/ScreenshotCommand';

export class ScreenshotBrowserCommand extends ScreenshotCommand {
  
  /**
   * Browser does ONE thing: capture screenshot with html2canvas
   * Then either returns data OR delegates to server for saving
   */
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    console.log(`📸 BROWSER: Capturing screenshot`);

    try {
      // Simple html2canvas capture
      const globalContext = (typeof window !== 'undefined' ? window : globalThis) as any;
      const html2canvas = globalContext.html2canvas;
      if (!html2canvas) {
        throw new Error('html2canvas not available');
      }

      const targetElement = params.selector 
        ? globalContext.document.querySelector(params.selector)
        : globalContext.document.body;
        
      if (!targetElement) {
        throw new Error(`Element not found: ${params.selector || 'body'}`);
      }

      // Build advanced html2canvas options
      const startTime = Date.now();
      const captureOptions = {
        height: globalContext.innerHeight || 600,
        width: globalContext.innerWidth || 800,
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

      console.log(`📷 BROWSER: Capturing ${params.selector || 'body'}`);
      const canvas = await html2canvas(targetElement, captureOptions);
      
      // Convert with specified format and quality
      const format = params.options?.format || 'png';
      const quality = params.options?.quality || 0.9;
      
      let dataUrl: string;
      if (format === 'jpeg') {
        dataUrl = canvas.toDataURL('image/jpeg', quality);
      } else if (format === 'webp') {
        dataUrl = canvas.toDataURL('image/webp', quality);
      } else {
        dataUrl = canvas.toDataURL('image/png');
      }
      
      const captureTime = Date.now() - startTime;
      console.log(`✅ BROWSER: Captured (${canvas.width}x${canvas.height}) in ${captureTime}ms`);
      
      // Create result with comprehensive metadata
      const result = new ScreenshotResult({
        success: true,
        filepath: '',
        filename: params.filename,
        environment: this.context.environment,
        timestamp: new Date().toISOString(),
        options: params.options,
        dataUrl: dataUrl,
        metadata: {
          width: canvas.width,
          height: canvas.height,
          size: dataUrl.length,
          selector: params.selector,
          format: format,
          captureTime: captureTime
        }
      });
      
      // Simple decision: if browser-initiated, send to server for saving
      if (!params.returnToSource) {
        console.log(`🔀 BROWSER: Sending to server for saving`);
        return await this.remoteExecute(result);
      }
      
      // Otherwise return to calling server
      console.log(`🔙 BROWSER: Returning data to server`);
      return result;

    } catch (error: any) {
      console.error(`❌ BROWSER: Failed:`, error.message);
      return new ScreenshotResult({
        success: false,
        filepath: '',
        filename: params.filename,
        environment: this.context.environment,
        timestamp: new Date().toISOString(),
        error: error.message
      });
    }
  }
}