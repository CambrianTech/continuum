/**
 * Screenshot Command - Browser Implementation
 * 
 * Browser-side screenshot command that uses html2canvas to capture screenshots
 * and returns the data to the requesting context.
 */

import { CommandBase, ICommandDaemon } from '../../../shared/CommandBase';
import { JTAGContext, JTAGPayload } from '../../../../../shared/JTAGTypes';
import { ScreenshotParams, ScreenshotResult, ScreenshotOptions } from '../shared/ScreenshotTypes';

export class ScreenshotBrowserCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot-browser', context, subpath, commander);
  }

  /**
   * Execute screenshot command in browser
   * Uses html2canvas to capture the page
   */
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    console.log(`📸 ${this.toString()}: Capturing screenshot in browser`);

    try {
      const startTime = Date.now();

      // Access global context safely (browser environment)
      const globalContext = (typeof window !== 'undefined' ? window : globalThis) as any;
      const html2canvas = globalContext.html2canvas;
      if (!html2canvas) {
        throw new Error('html2canvas not available - include script in page');
      }

      // Determine target element
      let targetElement = globalContext.document?.body;
      if (!targetElement) {
        throw new Error('Document body not available');
      }
      
      if (params.selector) {
        const element = globalContext.document?.querySelector(params.selector);
        if (!element) {
          throw new Error(`Element not found: ${params.selector}`);
        }
        targetElement = element as HTMLElement;
      }

      // Build html2canvas options with safe viewport access  
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

      // Capture screenshot
      console.log(`📷 ${this.toString()}: Capturing element: ${params.selector || 'body'}`);
      
      const canvas = await html2canvas(targetElement, captureOptions);

      // Convert to data URL with specified format and quality
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
      
      const result = new ScreenshotResult({
        success: true,
        filepath: '', // Browser doesn't save to file directly
        filename: params.filename,
        context: 'browser',
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

      console.log(`✅ ${this.toString()}: Screenshot captured (${canvas.width}x${canvas.height}) in ${captureTime}ms`);
      return result;

    } catch (error: any) {
      console.error(`❌ ${this.toString()}: Screenshot capture failed:`, error.message);
      
      return new ScreenshotResult({
        success: false,
        filepath: '',
        filename: params.filename,
        context: 'browser',
        timestamp: new Date().toISOString(),
        error: error.message
      });
    }
  }
}