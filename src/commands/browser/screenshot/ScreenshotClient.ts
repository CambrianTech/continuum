/**
 * Client-side screenshot capture functionality
 * Handles html2canvas integration and file saving
 */

interface ScreenshotClientParams {
  selector: string;
  filename: string;
  format: string;
  quality: number;
  animation: string;
  destination: string;
  directory: string;
}

interface ScreenshotResult {
  success: boolean;
  data?: {
    imageData: string;
    filename: string;
    selector: string;
    format: string;
    width: number;
    height: number;
    dataUrl: string;
    saved: boolean;
    filePath: string | null;
  };
  error?: string;
  timestamp: string;
  processor: string;
}

/**
 * Capture screenshot using html2canvas and save via continuum.fileSave
 */
export async function clientScreenshot(params: ScreenshotClientParams): Promise<ScreenshotResult> {
  console.log('📸 BROWSER: Starting html2canvas screenshot capture');
  console.log('📋 BROWSER: Params:', params);
  
  try {
    // Find target element
    const targetElement = params.selector === 'body' ? document.body : document.querySelector(params.selector);
    if (!targetElement) {
      throw new Error(`Element not found: ${params.selector}`);
    }
    
    // Load html2canvas dynamically if not already loaded
    const html2canvas = await loadHtml2Canvas();
    console.log('📦 BROWSER: html2canvas available - starting capture');
    
    // Capture screenshot
    const canvas = await html2canvas(targetElement, {
      allowTaint: true,
      useCORS: true,
      scale: 1,
      logging: false
    });
    
    // Convert to desired format
    const imageData = params.format === 'png' ? 
      canvas.toDataURL('image/png') : 
      canvas.toDataURL(`image/${params.format}`, params.quality);
    
    console.log('🖼️ BROWSER: Canvas captured, size:', canvas.width, 'x', canvas.height);
    
    // Extract base64 data from data URL
    const base64Data = imageData.replace(/^data:image\/[a-z]+;base64,/, '');
    
    // Convert base64 to Uint8Array for fileSave
    const bytes = Uint8Array.from(atob(base64Data), c => c.charCodeAt(0));
    
    console.log('💾 BROWSER: Calling fileSave with', bytes.length, 'bytes');
    
    // Save the file using continuum.fileSave
    const continuum = (window as any).continuum;
    const saveResult = await continuum.fileSave({
      content: bytes,
      filename: params.filename,
      artifactType: 'screenshot',
      directory: params.directory
    });
    
    console.log('💾 BROWSER: FileSave result:', saveResult);
    
    // Return result with screenshot data and save confirmation
    return {
      success: true,
      data: {
        imageData: base64Data,
        filename: params.filename,
        selector: params.selector,
        format: params.format,
        width: canvas.width,
        height: canvas.height,
        dataUrl: imageData,
        saved: saveResult.success,
        filePath: saveResult.data?.filePath || null
      },
      timestamp: new Date().toISOString(),
      processor: 'browser-html2canvas'
    };
    
  } catch (error) {
    console.error('❌ BROWSER: Screenshot capture failed:', error);
    return {
      success: false,
      error: error instanceof Error ? error.message : String(error),
      timestamp: new Date().toISOString(),
      processor: 'browser-html2canvas'
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

// Make function available globally for eval execution
(window as any).clientScreenshot = clientScreenshot;