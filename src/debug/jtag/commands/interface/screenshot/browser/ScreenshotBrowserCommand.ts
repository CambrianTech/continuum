/**
 * Screenshot Command - Browser Implementation (Simplified)
 * 
 * MINIMAL WORK PER COMMAND: Just implements what browser does
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ScreenshotParams, Html2CanvasCanvas, Html2CanvasOptions, ScreenshotResult, ScreenshotResolution } from '../shared/ScreenshotTypes';
import { createScreenshotResult, expandPresets, generateFilenameWithResolution, RESOLUTION_PRESETS } from '../shared/ScreenshotTypes';
import { EnhancementError } from '@system/core/types/ErrorTypes';
import { getGlobalAPI, getViewportDimensions } from '@daemons/command-daemon/shared/GlobalUtils';
import { 
  smartQuerySelector, 
  calculateCropCoordinates, 
  constrainCropToCanvas,
  getElementDisplayName 
} from '../shared/browser-utils/BrowserElementUtils';

const DEFAULT_FORMAT = 'png';
const DEFAULT_QUALITY = 0.9;

export class ScreenshotBrowserCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {
  static readonly commandName = 'screenshot';

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('interface/screenshot', context, subpath, commander);
  }
  
  /**
   * Multi-resolution screenshot capture
   */
  async captureAtResolution(
    params: ScreenshotParams, 
    resolution: ScreenshotResolution
  ): Promise<ScreenshotResult> {
    // Create resolution-specific params
    const resolutionParams = {
      ...params,
      width: resolution.width,
      height: resolution.height,
      scale: resolution.scale || params.scale,
      filename: params.filename ? generateFilenameWithResolution(params.filename, resolution) : undefined
    };
    
    return await this.executeSingleCapture(resolutionParams);
  }

  /**
   * Browser capture with advanced coordinate-based cropping
   * BREAKTHROUGH: Full body capture + element coordinate cropping (more reliable than html2canvas element capture)
   */
  async execute(params: ScreenshotParams): Promise<ScreenshotResult> {
    // console.debug(`🔍 BROWSER: Checking multi-resolution conditions:`);
    // console.debug(`   resolutions: ${params.options?.resolutions?.length || 0}`);
    // console.debug(`   presets: ${params.options?.presets?.length || 0}`);
    
    // Handle multi-resolution capture
    if (params.options?.resolutions?.length || params.options?.presets?.length) {
      // console.debug(`📐 BROWSER: Multi-resolution path selected`);
      return await this.executeMultiResolution(params);
    }
    
    // console.debug(`📸 BROWSER: Single capture path selected`);
    return await this.executeSingleCapture(params);
  }
  
  /**
   * Execute multi-resolution capture
   */
  async executeMultiResolution(params: ScreenshotParams): Promise<ScreenshotResult> {
    // console.debug(`📐 BROWSER: Multi-resolution capture requested`);
    
    // Combine custom resolutions and presets
    const customResolutions = params.options?.resolutions || [];
    const presetResolutions = params.options?.presets ? expandPresets(params.options.presets) : [];
    const allResolutions = [...customResolutions, ...presetResolutions];
    
    // console.debug(`📐 BROWSER: Capturing ${allResolutions.length} resolutions`);
    
    const results: ScreenshotResult[] = [];
    let firstResult: ScreenshotResult | null = null;
    
    for (const resolution of allResolutions) {
      // console.debug(`📐 BROWSER: Capturing ${resolution.width}x${resolution.height} (${resolution.suffix || 'custom'})`);
      const result = await this.captureAtResolution(params, resolution);
      results.push(result);
      
      // Use first successful result as primary return
      if (!firstResult && result.success) {
        firstResult = result;
      }
    }
    
    // Return the first successful result with metadata about all captures
    if (firstResult) {
      firstResult.metadata = {
        ...firstResult.metadata,
        multiResolution: true,
        resolutionCount: allResolutions.length,
        successfulCaptures: results.filter(r => r.success).length
      };
    }
    
    return firstResult || createScreenshotResult(params.context, params.sessionId, {
      success: false,
      error: new EnhancementError('multi-resolution', 'All resolution captures failed')
    });
  }

  /**
   * Single capture execution (original logic)
   */
  async executeSingleCapture(params: ScreenshotParams): Promise<ScreenshotResult> {
    // console.debug(`📸 BROWSER: Capturing screenshot`);
    // console.debug(`🔍 DEBUG: ScreenshotBrowserCommand.execute() CALLED with sessionId: ${params.sessionId}`);
    // console.debug(`🔍 DEBUG: Full params:`, JSON.stringify(params, null, 2));

    try {
      // Get html2canvas API with proper typing
      const html2canvas = getGlobalAPI<(element: Element, options?: Html2CanvasOptions) => Promise<Html2CanvasCanvas>>('html2canvas');
      if (!html2canvas) {
        throw new Error('html2canvas not available');
      }

      const startTime = Date.now();

      // IFRAME CAPTURE: If iframeSelector is specified, capture iframe content
      if (params.iframeSelector) {
        return await this.captureIframeContent(params, html2canvas, startTime);
      }

      // ADVANCED TARGETING: Use querySelector (modern) or selector (legacy)
      const targetSelector = params.querySelector || params.selector || 'body';
      const targetElement = targetSelector === 'body'
        ? document.body
        : smartQuerySelector(targetSelector);
        
      if (!targetElement) {
        throw new Error(`Element not found: ${targetSelector}`);
      }

      // Use modular element utilities for accurate bounds and coordinates
      const elementName = params.elementName || getElementDisplayName(targetElement);
      // console.debug(`🎯 BROWSER: Targeting element '${elementName}'`);

      // TEST: Try html2canvas direct element capture instead of coordinate cropping
      const scale = params.scale || params.options?.scale || 1;
      
      // CRITICAL FIX: Device pixel ratio normalization for consistent scaling
      const devicePixelRatio = window.devicePixelRatio || 1;
      // console.debug(`🖥️ BROWSER: Device pixel ratio: ${devicePixelRatio}, user scale: ${scale}`);
      
      // Force html2canvas to use device pixel ratio 1 for consistent scaling
      const captureOptions: Html2CanvasOptions = {
        scale: scale / devicePixelRatio, // Normalize for DPR
        scrollX: 0,
        scrollY: 0,
        useCORS: true,
        allowTaint: true,
        logging: false,
        backgroundColor: params.options?.backgroundColor,
        // Improved shadow and effect rendering
        foreignObjectRendering: true,
        removeContainer: true,
        ignoreElements: (element) => {
          // Skip problematic elements that cause shadow offset or hangs
          if (element.classList?.contains('html2canvas-ignore')) return true;
          // CRITICAL: Skip iframes to prevent html2canvas from hanging on cross-origin content
          if (element.tagName === 'IFRAME') return true;
          return false;
        },
        ...params.options?.html2canvasOptions
      };

      // CAPTURE STRATEGY: Choose between full body crop vs direct element capture
      // For shadow-heavy elements, direct capture may render shadows better
      const useDirectCapture = params.options?.directCapture || 
        (targetSelector !== 'body' && params.options?.preserveShadows);
      
      let canvas: HTMLCanvasElement;
      
      if (useDirectCapture && targetSelector !== 'body') {
        // console.debug(`📷 BROWSER: Direct element capture for better shadow rendering`);
        canvas = await html2canvas(targetElement, captureOptions) as HTMLCanvasElement;
      } else {
        // console.debug(`📷 BROWSER: Full body capture at scale ${scale}`);
        canvas = await html2canvas(document.body, captureOptions) as HTMLCanvasElement;
      }
      
      // console.debug(`📐 BROWSER: Canvas dimensions: ${canvas.width}x${canvas.height}`);
      
      // Calculate actual scaling factor from viewport to canvas (accounting for DPR)
      const viewport = getViewportDimensions();
      const actualScaleFactor = canvas.width / viewport.width;
      // console.debug(`📏 BROWSER: Canvas ${canvas.width}x${canvas.height}, viewport ${viewport.width}x${viewport.height}, scale factor: ${actualScaleFactor}`);
      
      // CROPPING LOGIC: Handle both direct capture and body crop strategies
      let finalCanvas = canvas;
      const needsCropping = !useDirectCapture && (targetSelector !== 'body' || params.cropX !== undefined || params.cropY !== undefined);
      
      // Declare crop variables outside the block for metadata access
      let cropX: number = 0, cropY: number = 0, cropWidth: number = canvas.width, cropHeight: number = canvas.height;
      
      if (needsCropping) {
        
        if (targetSelector !== 'body') {
          // For elements: Calculate coordinates from full body canvas
          const cropCoords = calculateCropCoordinates(targetElement, document.body, actualScaleFactor, true);
          const constrainedCrop = constrainCropToCanvas(cropCoords, canvas.width, canvas.height);
          
          cropX = constrainedCrop.x;
          cropY = constrainedCrop.y;
          cropWidth = constrainedCrop.width;
          cropHeight = constrainedCrop.height;
          
          // console.debug(`📏 BROWSER: Element coordinates: ${cropX},${cropY} ${cropWidth}x${cropHeight}`);
        } else {
          // For body with custom crop params
          cropX = (params.cropX || 0) * actualScaleFactor;
          cropY = (params.cropY || 0) * actualScaleFactor;
          cropWidth = (params.cropWidth || canvas.width) * actualScaleFactor;
          cropHeight = (params.cropHeight || canvas.height) * actualScaleFactor;
        }
        
        const croppedCanvas = document.createElement('canvas');
        const croppedCtx = croppedCanvas.getContext('2d')!;
        
        croppedCanvas.width = cropWidth;
        croppedCanvas.height = cropHeight;
        
        croppedCtx.drawImage(
          canvas,
          cropX, cropY, cropWidth, cropHeight,
          0, 0, cropWidth, cropHeight
        );
        
        finalCanvas = croppedCanvas;
        // console.debug(`✂️ BROWSER: Cropped from full body: ${cropX},${cropY} ${cropWidth}x${cropHeight}`);
      } else if (useDirectCapture) {
        // console.debug(`📷 BROWSER: Using direct element capture (shadows preserved)`);
      }
      
      // SCALING: Fit-inside behavior with aspect ratio preservation
      let targetWidth = finalCanvas.width;
      let targetHeight = finalCanvas.height;
      
      // Calculate scale factor to fit inside max dimensions while preserving aspect ratio
      if (params.width || params.height) {
        const maxWidth = params.width || finalCanvas.width;
        const maxHeight = params.height || finalCanvas.height;
        
        const scaleX = maxWidth / finalCanvas.width;
        const scaleY = maxHeight / finalCanvas.height;
        
        // Use the smaller scale factor to fit inside (preserve aspect ratio)
        const scaleFactor = Math.min(scaleX, scaleY, 1); // Don't scale up
        
        targetWidth = Math.round(finalCanvas.width * scaleFactor);
        targetHeight = Math.round(finalCanvas.height * scaleFactor);
        
        // console.debug(`📏 BROWSER: Fit-inside scaling: ${finalCanvas.width}x${finalCanvas.height} → ${targetWidth}x${targetHeight} (scale: ${scaleFactor.toFixed(3)})`);
      }
      
      // Apply scaling if different from capture size
      if (targetWidth !== finalCanvas.width || targetHeight !== finalCanvas.height) {
        const scaledCanvas = document.createElement('canvas');
        const scaledCtx = scaledCanvas.getContext('2d')!;
        
        scaledCanvas.width = targetWidth;
        scaledCanvas.height = targetHeight;
        
        scaledCtx.drawImage(finalCanvas, 0, 0, targetWidth, targetHeight);
        finalCanvas = scaledCanvas;
        
        // console.debug(`🔄 BROWSER: Scaled to ${targetWidth}x${targetHeight}`);
      }
      
      // QUALITY CONTROL: Convert with quality adjustment for file size limits
      const format = params.format || params.options?.format || DEFAULT_FORMAT;
      let quality = params.quality || params.options?.quality || DEFAULT_QUALITY;
      let dataUrl: string;
      let compressed = false;
      
      do {
        dataUrl = format === 'png' ? 
          finalCanvas.toDataURL('image/png') : 
          finalCanvas.toDataURL(`image/${format}`, quality);
        
        // Check file size if maxFileSize specified
        if (params.maxFileSize) {
          const estimatedSize = (dataUrl.length * 3) / 4; // Base64 to bytes estimate
          if (estimatedSize > params.maxFileSize && quality > 0.1) {
            quality -= 0.1;
            compressed = true;
            // console.debug(`📉 BROWSER: Reducing quality to ${quality} for file size limit`);
            continue;
          }
        }
        break;
      } while (true);
      
      const captureTime = Date.now() - startTime;
      // console.debug(`✅ BROWSER: Captured (${finalCanvas.width}x${finalCanvas.height}) in ${captureTime}ms`);
      
      // Enrich params with advanced metadata
      params.dataUrl = dataUrl;
      params.metadata = {
        originalWidth: canvas.width,
        originalHeight: canvas.height,
        width: finalCanvas.width,
        height: finalCanvas.height,
        fileSizeBytes: Math.floor((dataUrl.length * 3) / 4),
        size: dataUrl.length,
        selector: targetSelector,
        elementName: elementName,
        format: format,
        captureTime: captureTime,
        scale: scale,
        quality: quality,
        cropped: needsCropping,
        cropCoordinates: needsCropping ? { x: cropX, y: cropY, width: cropWidth, height: cropHeight } : undefined,
        compressed: compressed
      };

      if (params.resultType === 'file' || params.destination === 'file' || params.destination === 'both') {
        params.filename = params.filename ?? `screenshot-${elementName}-${Date.now()}.${format}`;
      }

      // For file output or cross-context, delegate to server
      if (params.resultType === 'file' || params.destination === 'file' || params.destination === 'both' || 
          params.context.uuid !== this.context.uuid) {
        return await this.remoteExecute(params);
      }

      // Return bytes directly to caller (same context)
      const base64Data = dataUrl.replace(/^data:image\/[a-z]+;base64,/, '');
      const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));
      
      return createScreenshotResult(
        params.context,
        params.sessionId,
        {
          success: true,
          dataUrl: dataUrl,
          options: params.options,
          metadata: params.metadata,
          // Include bytes for 'bytes' or 'both' destinations
          bytes: (params.destination === 'bytes' || params.destination === 'both') ? bytes : undefined
        }
      );

    } catch (error: any) {
      console.error(`❌ BROWSER: Failed:`, error.message);
      return createScreenshotResult(params.context, params.sessionId, {
        success: false,
        error: new EnhancementError('screenshot-capture', error.message || 'Screenshot capture failed')
      });
    }
  }

  /**
   * Capture content inside an iframe using injected JTAG shim
   * The shim runs html2canvas INSIDE the iframe (no CORS issues)
   */
  private async captureIframeContent(
    params: ScreenshotParams,
    _html2canvas: (element: Element, options?: Html2CanvasOptions) => Promise<Html2CanvasCanvas>,
    startTime: number
  ): Promise<ScreenshotResult> {
    try {
      // Find the iframe using smart selector (pierces shadow DOM)
      const iframe = smartQuerySelector(params.iframeSelector!) as HTMLIFrameElement;
      if (!iframe) {
        throw new Error(`Iframe not found: ${params.iframeSelector}`);
      }

      if (!(iframe instanceof HTMLIFrameElement)) {
        throw new Error(`Element is not an iframe: ${params.iframeSelector}`);
      }

      console.log(`📷 BROWSER: Capturing iframe via JTAG shim from ${params.iframeSelector}`);

      // Send screenshot command to injected JTAG shim via postMessage
      const requestId = `screenshot-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;

      const shimResult = await new Promise<{
        success: boolean;
        data?: {
          dataUrl: string;
          metadata: { width: number; height: number; format: string; quality: number };
        };
        error?: { message: string };
      }>((resolve) => {
        const timeout = setTimeout(() => {
          window.removeEventListener('message', handler);
          resolve({ success: false, error: { message: 'JTAG shim timeout - shim may not be loaded' } });
        }, 30000); // 30s timeout for capture

        const handler = (event: MessageEvent) => {
          if (event.data?.type === 'jtag-shim-response' && event.data?.requestId === requestId) {
            clearTimeout(timeout);
            window.removeEventListener('message', handler);
            resolve(event.data.result);
          }
        };

        window.addEventListener('message', handler);

        // Send request to iframe's JTAG shim
        // Default to viewport-only for iframe screenshots (prevents huge images overwhelming AIs)
        iframe.contentWindow?.postMessage({
          type: 'jtag-shim-request',
          command: 'screenshot',
          requestId,
          params: {
            selector: params.querySelector,
            scale: params.scale || params.options?.scale || 1,
            format: params.format || params.options?.format || 'png',
            quality: params.quality || params.options?.quality || 0.9,
            backgroundColor: params.options?.backgroundColor || '#ffffff',
            viewportOnly: params.viewportOnly !== false  // Default true for iframes
          }
        }, '*');
      });

      if (!shimResult.success || !shimResult.data?.dataUrl) {
        throw new Error(shimResult.error?.message || 'Shim capture failed');
      }

      const { dataUrl, metadata } = shimResult.data;
      const captureTime = Date.now() - startTime;
      console.log(`✅ BROWSER: Iframe captured via shim (${metadata?.width}x${metadata?.height}) in ${captureTime}ms`);

      // Set up params for server handoff
      const format = params.format || params.options?.format || 'png';
      params.dataUrl = dataUrl;
      params.metadata = {
        width: metadata?.width,
        height: metadata?.height,
        fileSizeBytes: Math.floor((dataUrl.length * 3) / 4),
        size: dataUrl.length,
        selector: params.iframeSelector,
        elementName: 'iframe-content-via-shim',
        format: format,
        captureTime: captureTime,
        scale: params.scale || params.options?.scale || 1,
        quality: metadata?.quality
      };

      if (params.resultType === 'file' || params.destination === 'file' || params.destination === 'both') {
        params.filename = params.filename ?? `iframe-screenshot-${Date.now()}.${format}`;
      }

      // Delegate to server for file saving
      if (params.resultType === 'file' || params.destination === 'file' || params.destination === 'both' ||
          params.context.uuid !== this.context.uuid) {
        return await this.remoteExecute(params);
      }

      // Return bytes directly
      const base64Data = dataUrl.replace(/^data:image\/[a-z]+;base64,/, '');
      const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));

      return createScreenshotResult(params.context, params.sessionId, {
        success: true,
        dataUrl: dataUrl,
        options: params.options,
        metadata: params.metadata,
        bytes: (params.destination === 'bytes' || params.destination === 'both') ? bytes : undefined
      });

    } catch (error: any) {
      console.error(`❌ BROWSER: Iframe shim capture failed:`, error.message);
      return createScreenshotResult(params.context, params.sessionId, {
        success: false,
        error: new EnhancementError('iframe-capture', error.message || 'Iframe screenshot capture failed')
      });
    }
  }
}