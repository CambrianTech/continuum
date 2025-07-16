/**
 * ScreenshotBase - Shared foundation for all screenshot functionality
 * 
 * Following middle-out architecture pattern:
 * - Centralized html2canvas management
 * - Common validation and processing logic
 * - Shared error handling and logging
 * - Unified element targeting system
 */

import { ScreenshotFormat } from '../ScreenshotTypes';

export interface ScreenshotBaseOptions {
  selector?: string;
  querySelector?: string;
  filename?: string;
  format?: ScreenshotFormat;
  quality?: number;
  width?: number;
  height?: number;
  scale?: number;
  cropX?: number;
  cropY?: number;
  cropWidth?: number;
  cropHeight?: number;
  elementName?: string;
  maxFileSize?: number;
}

export interface ScreenshotCaptureResult {
  imageData: string;
  format: ScreenshotFormat;
  selector: string;
  filename: string;
  width: number;
  height: number;
  elementName: string;
  originalWidth: number;
  originalHeight: number;
  scale: number;
  cropped: boolean;
  compressed: boolean;
  fileSizeBytes: number;
  dataUrl: string;
}

/**
 * Abstract base class for screenshot implementations
 * Provides common functionality for client and server contexts
 */
export abstract class ScreenshotBase {
  protected static html2canvasLoaded = false;
  protected static html2canvasPromise: Promise<any> | null = null;

  /**
   * Dynamically load html2canvas library (shared across all instances)
   */
  protected static async loadHtml2Canvas(): Promise<any> {
    if (typeof window === 'undefined') {
      throw new Error('html2canvas is only available in browser context');
    }

    // Return cached instance if already loaded
    if ((window as any).html2canvas) {
      return (window as any).html2canvas;
    }

    // Return existing promise if already loading
    if (this.html2canvasPromise) {
      return this.html2canvasPromise;
    }

    // Create new loading promise
    this.html2canvasPromise = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
      script.onload = () => {
        console.log('✅ html2canvas loaded successfully');
        this.html2canvasLoaded = true;
        const html2canvas = (window as any).html2canvas;
        if (html2canvas) {
          resolve(html2canvas);
        } else {
          reject(new Error('html2canvas not available after load'));
        }
      };
      script.onerror = () => {
        reject(new Error('Failed to load html2canvas library'));
      };
      document.head.appendChild(script);
    });

    return this.html2canvasPromise;
  }

  /**
   * Find target element with fallback logic
   */
  protected static findTargetElement(options: ScreenshotBaseOptions): Element {
    const selector = options.querySelector || options.selector || 'body';
    
    if (selector === 'body') {
      return document.body;
    }

    const element = document.querySelector(selector);
    if (!element) {
      throw new Error(`Screenshot target not found: ${selector}`);
    }

    return element;
  }

  /**
   * Get element name for logging (simplified)
   */
  protected static getElementName(element: Element): string {
    // Return ID if available (most specific)
    if (element.id) return `#${element.id}`;
    
    // Return first class if available
    if (element.classList.length > 0) return `.${element.classList[0]}`;
    
    // Otherwise return the tag name
    return element.tagName.toLowerCase();
  }

  /**
   * Validate screenshot parameters
   */
  protected static validateOptions(options: ScreenshotBaseOptions): void {
    if (options.scale && (options.scale < 0.1 || options.scale > 2.0)) {
      throw new Error('Scale must be between 0.1 and 2.0');
    }

    if (options.quality && (options.quality < 0.1 || options.quality > 1.0)) {
      throw new Error('Quality must be between 0.1 and 1.0');
    }

    if (options.width && options.width <= 0) {
      throw new Error('Width must be positive');
    }

    if (options.height && options.height <= 0) {
      throw new Error('Height must be positive');
    }
  }

  /**
   * Process canvas with AI-friendly features: scaling, cropping, compression
   */
  protected static processCanvas(
    canvas: HTMLCanvasElement, 
    options: ScreenshotBaseOptions
  ): HTMLCanvasElement {
    const processedCanvas = document.createElement('canvas');
    const ctx = processedCanvas.getContext('2d')!;
    
    // Determine final dimensions
    let sourceX = options.cropX || 0;
    let sourceY = options.cropY || 0;
    let sourceWidth = options.cropWidth || canvas.width;
    let sourceHeight = options.cropHeight || canvas.height;
    
    // Clamp crop dimensions to canvas bounds
    sourceX = Math.max(0, Math.min(sourceX, canvas.width));
    sourceY = Math.max(0, Math.min(sourceY, canvas.height));
    sourceWidth = Math.min(sourceWidth, canvas.width - sourceX);
    sourceHeight = Math.min(sourceHeight, canvas.height - sourceY);
    
    // Apply target dimensions (scale down if needed)
    let targetWidth = options.width || sourceWidth;
    let targetHeight = options.height || sourceHeight;
    
    // Maintain aspect ratio if only one dimension specified
    if (options.width && !options.height) {
      targetHeight = (sourceHeight * options.width) / sourceWidth;
    } else if (options.height && !options.width) {
      targetWidth = (sourceWidth * options.height) / sourceHeight;
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
   * Generate filename with timestamp if not provided
   */
  protected static generateFilename(
    options: ScreenshotBaseOptions,
    format: ScreenshotFormat = ScreenshotFormat.PNG
  ): string {
    if (options.filename) {
      return options.filename;
    }
    
    const timestamp = Date.now();
    const selector = options.querySelector || options.selector || 'body';
    const elementName = selector.replace(/[^a-zA-Z0-9]/g, '-');
    
    return `screenshot-${elementName}-${timestamp}.${format}` as string;
  }

  /**
   * Abstract method for executing screenshot capture
   * Must be implemented by concrete classes
   */
  protected abstract executeCapture(options: ScreenshotBaseOptions): Promise<ScreenshotCaptureResult>;
}