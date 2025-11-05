/**
 * Unified Screenshot Client - Browser-side screenshot capture
 * 
 * Combines functionality from:
 * - Original ScreenshotClient.ts (direct function calls)
 * - ScreenshotExecutor.ts (remote execution events)
 * 
 * Following middle-out architecture pattern:
 * - Extends ScreenshotBase for shared functionality
 * - Handles both direct calls and remote execution
 * - Maintains backward compatibility
 */

import { ScreenshotBase, ScreenshotBaseOptions, ScreenshotCaptureResult } from '../shared/ScreenshotBase';
import type { ScreenshotClientRequest, ScreenshotResult, ScreenshotFormat } from '../shared/ScreenshotTypes';
import { RemoteExecutionResponse } from '../../../../ui/continuum-browser-client/types/WebSocketTypes';

interface ScreenshotClientParams extends ScreenshotClientRequest {
  // Directory will be determined by session context, not passed explicitly
}

/**
 * Unified Screenshot Client Class
 * Handles both direct function calls and remote execution events
 */
export class ScreenshotClient extends ScreenshotBase {
  private static instance: ScreenshotClient | null = null;
  private isRegistered = false;

  /**
   * Singleton pattern for global event handler
   */
  static getInstance(): ScreenshotClient {
    if (!ScreenshotClient.instance) {
      ScreenshotClient.instance = new ScreenshotClient();
    }
    return ScreenshotClient.instance;
  }

  /**
   * Register screenshot handler with WebSocket manager for remote execution
   */
  register(): void {
    if (this.isRegistered) return;

    document.addEventListener('continuum:remote_execution', this.handleRemoteExecution.bind(this) as EventListener);
    this.isRegistered = true;
    console.log('📸 ScreenshotClient registered for remote execution');
  }

  /**
   * Handle remote execution events - only respond to SCREENSHOT commands
   */
  private async handleRemoteExecution(event: Event): Promise<void> {
    const customEvent = event as CustomEvent;
    const { request, respond } = customEvent.detail;
    
    if (request.command.toUpperCase() !== 'SCREENSHOT') {
      return; // Not our command, let other handlers deal with it
    }

    const startTime = Date.now();
    console.log('📸 ScreenshotClient handling remote screenshot request', request);

    try {
      const result = await this.executeCapture(request.params);
      
      const response: RemoteExecutionResponse = {
        success: true,
        data: result,
        requestId: request.requestId,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime: Date.now() - startTime
        }
      };

      respond(response);

    } catch (error) {
      console.error('❌ Remote screenshot execution failed:', error);
      
      const response: RemoteExecutionResponse = {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        requestId: request.requestId,
        clientMetadata: {
          userAgent: navigator.userAgent,
          timestamp: Date.now(),
          executionTime: Date.now() - startTime
        }
      };

      respond(response);
    }
  }

  /**
   * Direct screenshot capture function (maintains backward compatibility)
   */
  async captureScreenshot(params: ScreenshotClientParams): Promise<ScreenshotResult> {
    console.log('📸 BROWSER: Starting AI-enhanced screenshot capture');
    console.log('📋 BROWSER: Params:', params);
    
    try {
      const result = await this.executeCapture(params as ScreenshotBaseOptions);
      
      // Convert base64 to Uint8Array for fileSave
      const base64Data = result.imageData;
      const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));
      
      console.log(`💾 BROWSER: Final image: ${bytes.length} bytes`);
      
      // Save the file using continuum.fileSave
      const continuum = (window as any).continuum;
      const saveResult = await continuum.fileSave({
        content: bytes,
        filename: params.filename,
        artifactType: 'screenshot'
      });
      
      console.log('💾 BROWSER: FileSave result:', saveResult);
      
      // Return AI-friendly result with comprehensive metadata
      return {
        success: true,
        data: {
          ...result,
          saved: saveResult.success,
          filePath: saveResult.data?.filePath || null,
          fullPath: saveResult.data?.fullPath || null,
          relativePath: saveResult.data?.relativePath || null,
          bytes: (params.destination === 'bytes' || params.destination === 'both') ? bytes : undefined
        },
        timestamp: new Date().toISOString(),
        processor: 'browser-html2canvas-ai'
      };
      
    } catch (error) {
      console.error('❌ BROWSER: Screenshot capture failed:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error),
        timestamp: new Date().toISOString(),
        processor: 'browser-html2canvas-ai'
      };
    }
  }

  /**
   * Execute screenshot capture using html2canvas (implements abstract method)
   */
  protected async executeCapture(options: ScreenshotBaseOptions): Promise<ScreenshotCaptureResult> {
    // Validate parameters
    ScreenshotBase.validateOptions(options);

    // Find target element
    const targetElement = ScreenshotBase.findTargetElement(options);
    const elementRect = targetElement.getBoundingClientRect();
    const elementName = options.elementName || ScreenshotBase.getElementName(targetElement);
    
    console.log(`🎯 BROWSER: Targeting element '${elementName}' at ${elementRect.width}x${elementRect.height}`);
    
    // Load html2canvas dynamically if not already loaded
    const html2canvas = await ScreenshotBase.loadHtml2Canvas();
    console.log('📦 BROWSER: html2canvas available - starting capture');
    
    // Always capture full body for consistent color rendering
    const scale = options.scale || 1;
    const canvas = await html2canvas(document.body, {
      allowTaint: true,
      useCORS: true,
      scale: scale,
      logging: false
    });
    
    // Calculate element coordinates relative to body for cropping
    const bodyRect = document.body.getBoundingClientRect();
    const relativeX = Math.max(0, (elementRect.left - bodyRect.left) * scale);
    const relativeY = Math.max(0, (elementRect.top - bodyRect.top) * scale);
    const relativeWidth = Math.min(elementRect.width * scale, canvas.width - relativeX);
    const relativeHeight = Math.min(elementRect.height * scale, canvas.height - relativeY);
    
    console.log(`📏 BROWSER: Element coordinates: ${relativeX},${relativeY} ${relativeWidth}x${relativeHeight}`);
    
    // For body captures, skip cropping to maintain full page
    const selector = options.querySelector || options.selector || 'body';
    const needsCropping = selector !== 'body';
    
    // Create cropped canvas if targeting specific element
    let finalCanvas = canvas;
    if (needsCropping) {
      const croppedCanvas = document.createElement('canvas');
      const croppedCtx = croppedCanvas.getContext('2d')!;
      
      croppedCanvas.width = relativeWidth;
      croppedCanvas.height = relativeHeight;
      
      croppedCtx.drawImage(
        canvas,
        relativeX, relativeY, relativeWidth, relativeHeight,
        0, 0, relativeWidth, relativeHeight
      );
      
      finalCanvas = croppedCanvas;
      console.log(`✂️ BROWSER: Cropped to element coordinates`);
    }
    
    const originalWidth = finalCanvas.width;
    const originalHeight = finalCanvas.height;
    
    console.log(`🖼️ BROWSER: Canvas captured, original size: ${originalWidth}x${originalHeight}`);
    
    // Apply AI-friendly processing
    const processedCanvas = ScreenshotBase.processCanvas(finalCanvas, options);
    const finalWidth = processedCanvas.width;
    const finalHeight = processedCanvas.height;
    
    console.log(`🎨 BROWSER: Processing complete, final size: ${finalWidth}x${finalHeight}`);
    
    // Convert to desired format with quality control
    const format = options.format || 'png';
    let quality = options.quality || 0.9;
    let imageData: string;
    
    do {
      imageData = format === 'png' ? 
        processedCanvas.toDataURL('image/png') : 
        processedCanvas.toDataURL(`image/${format}`, quality);
      
      // Check file size if maxFileSize is specified
      if (options.maxFileSize) {
        const estimatedSize = (imageData.length * 3) / 4; // Base64 to bytes estimate
        if (estimatedSize > options.maxFileSize && quality > 0.1) {
          quality -= 0.1;
          console.log(`📉 BROWSER: Reducing quality to ${quality} for file size limit`);
          continue;
        }
      }
      break;
    } while (true);
    
    // Extract base64 data from data URL
    const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
    const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));
    
    // Generate filename
    const filename = ScreenshotBase.generateFilename(options, format as ScreenshotFormat);
    
    console.log(`✅ Screenshot captured: ${bytes.length} bytes`);

    return {
      imageData: base64Data,
      format: format as ScreenshotFormat,
      selector: selector,
      filename: filename,
      width: finalWidth,
      height: finalHeight,
      elementName: elementName,
      originalWidth: originalWidth,
      originalHeight: originalHeight,
      scale: scale,
      cropped: needsCropping || !!(options.cropX || options.cropY || options.cropWidth || options.cropHeight),
      compressed: quality < 0.9,
      fileSizeBytes: bytes.length,
      dataUrl: imageData
    };
  }
}

/**
 * Backward compatibility function - maintains existing API
 */
export async function clientScreenshot(params: ScreenshotClientParams): Promise<ScreenshotResult> {
  const client = ScreenshotClient.getInstance();
  return await client.captureScreenshot(params);
}

// Make functions available globally for eval execution
(window as any).clientScreenshot = clientScreenshot;
(window as any).ScreenshotClient = ScreenshotClient;

// Auto-register when module loads (maintains ScreenshotExecutor behavior)
ScreenshotClient.getInstance().register();