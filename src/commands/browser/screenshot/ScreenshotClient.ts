/**
 * Client-side screenshot capture functionality
 * Handles html2canvas integration and file saving
 */

import type { ScreenshotClientRequest, ScreenshotResult } from './ScreenshotTypes';

interface ScreenshotClientParams extends ScreenshotClientRequest {
  // Directory will be determined by session context, not passed explicitly
}

/**
 * Capture screenshot using html2canvas with AI-friendly features
 * Supports element targeting, scaling, cropping, and compression
 */
export async function clientScreenshot(params: ScreenshotClientParams): Promise<ScreenshotResult> {
  console.log('📸 BROWSER: Starting AI-enhanced screenshot capture');
  console.log('📋 BROWSER: Params:', params);
  
  try {
    // Find target element - prioritize querySelector over selector
    const targetSelector = params.querySelector || params.selector;
    const targetElement = targetSelector === 'body' ? document.body : document.querySelector(targetSelector);
    if (!targetElement) {
      throw new Error(`Element not found: ${targetSelector}`);
    }
    
    // Get element info for AI context
    const elementRect = targetElement.getBoundingClientRect();
    const elementName = params.elementName || getElementName(targetElement);
    
    console.log(`🎯 BROWSER: Targeting element '${elementName}' at ${elementRect.width}x${elementRect.height}`);
    
    // Load html2canvas dynamically if not already loaded
    const html2canvas = await loadHtml2Canvas();
    console.log('📦 BROWSER: html2canvas available - starting capture');
    
    // Always capture full body for consistent color rendering
    const scale = params.scale || 1;
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
    const needsCropping = targetSelector !== 'body';
    
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
    const processedCanvas = await processCanvas(finalCanvas, params);
    const finalWidth = processedCanvas.width;
    const finalHeight = processedCanvas.height;
    
    console.log(`🎨 BROWSER: Processing complete, final size: ${finalWidth}x${finalHeight}`);
    
    // Convert to desired format with quality control
    let imageData: string;
    let quality = params.quality || 0.9;
    
    do {
      imageData = params.format === 'png' ? 
        processedCanvas.toDataURL('image/png') : 
        processedCanvas.toDataURL(`image/${params.format}`, quality);
      
      // Check file size if maxFileSize is specified
      if (params.maxFileSize) {
        const estimatedSize = (imageData.length * 3) / 4; // Base64 to bytes estimate
        if (estimatedSize > params.maxFileSize && quality > 0.1) {
          quality -= 0.1;
          console.log(`📉 BROWSER: Reducing quality to ${quality} for file size limit`);
          continue;
        }
      }
      break;
    } while (true);
    
    // Extract base64 data from data URL
    const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
    
    // Convert base64 to Uint8Array for fileSave
    const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));
    
    console.log(`💾 BROWSER: Final image: ${bytes.length} bytes, quality: ${quality}`);
    
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
        imageData: base64Data,
        filename: params.filename,
        selector: targetSelector,
        format: params.format,
        width: finalWidth,
        height: finalHeight,
        elementName: elementName,
        originalWidth: originalWidth,
        originalHeight: originalHeight,
        scale: scale,
        cropped: !!(params.cropX || params.cropY || params.cropWidth || params.cropHeight),
        compressed: quality < 0.9,
        fileSizeBytes: bytes.length,
        dataUrl: imageData,
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
 * Dynamically load html2canvas library
 */
async function loadHtml2Canvas(): Promise<any> {
  return new Promise((resolve, reject) => {
    // Check if already loaded
    if ((window as any).html2canvas) {
      resolve((window as any).html2canvas);
      return;
    }

    const script = document.createElement('script');
    script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
    script.onload = (): void => {
      console.log('✅ html2canvas loaded successfully');
      const html2canvas = (window as any).html2canvas;
      if (html2canvas) {
        resolve(html2canvas);
      } else {
        reject(new Error('html2canvas not available after load'));
      }
    };
    script.onerror = (): void => {
      reject(new Error('Failed to load html2canvas'));
    };
    document.head.appendChild(script);
  });
}

/**
 * Process canvas with AI-friendly features: scaling, cropping, compression
 */
async function processCanvas(canvas: HTMLCanvasElement, params: ScreenshotClientParams): Promise<HTMLCanvasElement> {
  const processedCanvas = document.createElement('canvas');
  const ctx = processedCanvas.getContext('2d')!;
  
  // Determine final dimensions
  let sourceX = params.cropX || 0;
  let sourceY = params.cropY || 0;
  let sourceWidth = params.cropWidth || canvas.width;
  let sourceHeight = params.cropHeight || canvas.height;
  
  // Clamp crop dimensions to canvas bounds
  sourceX = Math.max(0, Math.min(sourceX, canvas.width));
  sourceY = Math.max(0, Math.min(sourceY, canvas.height));
  sourceWidth = Math.min(sourceWidth, canvas.width - sourceX);
  sourceHeight = Math.min(sourceHeight, canvas.height - sourceY);
  
  // Apply target dimensions (scale down if needed)
  let targetWidth = params.width || sourceWidth;
  let targetHeight = params.height || sourceHeight;
  
  // Maintain aspect ratio if only one dimension specified
  if (params.width && !params.height) {
    targetHeight = (sourceHeight * params.width) / sourceWidth;
  } else if (params.height && !params.width) {
    targetWidth = (sourceWidth * params.height) / sourceHeight;
  }
  
  // Don't scale up, only down for AI efficiency
  targetWidth = Math.min(targetWidth, sourceWidth);
  targetHeight = Math.min(targetHeight, sourceHeight);
  
  processedCanvas.width = targetWidth;
  processedCanvas.height = targetHeight;
  
  // Draw the processed image
  ctx.drawImage(
    canvas,
    sourceX, sourceY, sourceWidth, sourceHeight,
    0, 0, targetWidth, targetHeight
  );
  
  return processedCanvas;
}

/**
 * Get human-readable element name for AI context
 */
function getElementName(element: Element): string {
  // Try to get a meaningful name for the element
  if (element.id) return `#${element.id}`;
  if (element.classList.length > 0) return `.${element.classList[0]}`;
  if (element.tagName === 'BODY') return 'page';
  if (element.tagName === 'MAIN') return 'main-content';
  if (element.tagName === 'HEADER') return 'header';
  if (element.tagName === 'FOOTER') return 'footer';
  if (element.tagName === 'NAV') return 'navigation';
  if (element.tagName === 'ASIDE') return 'sidebar';
  if (element.tagName === 'ARTICLE') return 'article';
  if (element.tagName === 'SECTION') return 'section';
  
  // For form elements
  if (element.tagName === 'FORM') return 'form';
  if (element.tagName === 'INPUT') return 'input-field';
  if (element.tagName === 'BUTTON') return 'button';
  if (element.tagName === 'SELECT') return 'dropdown';
  if (element.tagName === 'TEXTAREA') return 'text-area';
  
  // For content elements
  if (element.tagName === 'H1') return 'heading-1';
  if (element.tagName === 'H2') return 'heading-2';
  if (element.tagName === 'H3') return 'heading-3';
  if (element.tagName === 'P') return 'paragraph';
  if (element.tagName === 'UL') return 'list';
  if (element.tagName === 'TABLE') return 'table';
  if (element.tagName === 'IMG') return 'image';
  
  return element.tagName.toLowerCase();
}

// Make function available globally for eval execution
(window as any).clientScreenshot = clientScreenshot;