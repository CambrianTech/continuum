/**
 * JTAG Screenshot Module
 * 
 * Handles screenshot functionality for both browser and server contexts
 */

import { JTAGScreenshotOptions, JTAGScreenshotResult, JTAGConfig } from '../JTAGTypes';

export class JTAGScreenshot {
  
  /**
   * Take screenshot - delegates to appropriate context handler
   */
  static async screenshot(
    filename: string = '',
    options: JTAGScreenshotOptions = {},
    timestamp: string,
    isClient: boolean,
    config: JTAGConfig,
    log: (component: string, message: string, data?: any) => void,
    captureBrowserScreenshot?: (filename: string, options: JTAGScreenshotOptions, timestamp: string) => Promise<JTAGScreenshotResult>,
    captureServerScreenshot?: (filename: string, options: JTAGScreenshotOptions, timestamp: string) => Promise<JTAGScreenshotResult>
  ): Promise<JTAGScreenshotResult> {
    
    const screenshotName = filename || `jtag-debug-${Date.now()}`;
    
    log('JTAG', `Screenshot requested via JTAG`, { 
      context: config.context,
      filename: screenshotName,
      options 
    });
    
    if (isClient) {
      if (!captureBrowserScreenshot) {
        throw new Error('Browser screenshot handler not available');
      }
      return await captureBrowserScreenshot(screenshotName, options, timestamp);
    } else {
      if (!captureServerScreenshot) {
        throw new Error('Server screenshot handler not available');
      }
      return await captureServerScreenshot(screenshotName, options, timestamp);
    }
  }

  /**
   * Browser screenshot implementation
   */
  static async captureBrowserScreenshot(
    filename: string,
    options: JTAGScreenshotOptions,
    timestamp: string,
    config: JTAGConfig,
    loadHtml2Canvas: () => Promise<any>,
    log: (component: string, message: string, data?: any) => void,
    critical: (component: string, message: string, data?: any) => void,
    sendWebSocketMessage: (type: string, payload: any) => Promise<any>
  ): Promise<JTAGScreenshotResult> {
    
    try {
      log('JTAG_BROWSER_SCREENSHOT', 'Starting browser screenshot capture', { filename, options });
      
      // Load html2canvas library
      const html2canvas = await loadHtml2Canvas();
      
      // Get target element
      const selector = options.selector || 'body';
      const targetElement = document.querySelector(selector);
      
      if (!targetElement) {
        const error = `Element not found: ${selector}`;
        critical('JTAG_BROWSER_SCREENSHOT', error, { selector });
        return {
          success: false,
          error,
          filepath: '',
          filename,
          context: 'browser',
          timestamp,
          metadata: { selector, found: false }
        };
      }

      // Configure html2canvas options
      const html2canvasOptions = {
        width: options.width || 1200,
        height: options.height || 800,
        scale: options.scale || 1,
        useCORS: true,
        allowTaint: false,
        backgroundColor: options.backgroundColor || '#ffffff',
        logging: false,
        ...options.html2canvasOptions
      };

      log('JTAG_BROWSER_SCREENSHOT', 'Capturing element with html2canvas', html2canvasOptions);
      
      // Capture screenshot
      const canvas = await html2canvas(targetElement, html2canvasOptions);
      const dataURL = canvas.toDataURL('image/png');
      
      // Send to server for saving
      const serverResult = await sendWebSocketMessage('screenshot', {
        filename,
        dataURL,
        options: html2canvasOptions,
        timestamp,
        selector
      });

      if (serverResult.success) {
        log('JTAG_BROWSER_SCREENSHOT', 'Screenshot captured and saved', {
          filename: serverResult.filename,
          size: dataURL.length
        });
        
        return {
          success: true,
          filepath: serverResult.filepath || `screenshots/${filename}.png`,
          filename: serverResult.filename || filename,
          context: 'browser',
          timestamp,
          metadata: {
            width: canvas.width,
            height: canvas.height,
            selector,
            size: dataURL.length
          }
        };
      } else {
        return {
          success: false,
          error: serverResult.error || 'Failed to save screenshot',
          filepath: '',
          filename,
          context: 'browser',
          timestamp
        };
      }
      
    } catch (error: any) {
      critical('JTAG_BROWSER_SCREENSHOT', 'Screenshot capture failed', { error: error.message });
      return {
        success: false,
        error: error.message,
        filepath: '',
        filename,
        context: 'browser',
        timestamp
      };
    }
  }

  /**
   * Server screenshot implementation  
   */
  static async captureServerScreenshot(
    filename: string,
    options: JTAGScreenshotOptions,
    timestamp: string,
    config: JTAGConfig,
    log: (component: string, message: string, data?: any) => void
  ): Promise<JTAGScreenshotResult> {
    
    // Server-side screenshots are placeholders for now
    // In a full implementation, this would use Puppeteer or similar
    
    const screenshotDir = config.screenshotDirectory || `${config.rootPath}/screenshots`;
    const filepath = `${screenshotDir}/${filename}.txt`;
    
    const placeholderContent = `JTAG Server Screenshot Placeholder
Filename: ${filename}
Timestamp: ${timestamp}
Options: ${JSON.stringify(options, null, 2)}
Context: server

This is a placeholder for server-side screenshot functionality.
In production, this would use browser automation tools like Puppeteer
to capture actual screenshots of running applications.
`;
    
    try {
      // Create directory if needed
      const fs = require('fs');
      const path = require('path');
      
      if (!fs.existsSync(screenshotDir)) {
        fs.mkdirSync(screenshotDir, { recursive: true });
      }
      
      // Write placeholder file
      fs.writeFileSync(filepath, placeholderContent);
      
      const stats = fs.statSync(filepath);
      
      log('JTAG', 'Server screenshot placeholder created', {
        width: options.width || 1024,
        height: options.height || 768,
        size: stats.size,
        selector: options.selector || 'body'
      });
      
      return {
        success: true,
        filepath,
        filename: `${filename}.txt`,
        context: 'server',
        timestamp,
        metadata: {
          width: options.width || 1024,
          height: options.height || 768,
          size: stats.size,
          isPlaceholder: true
        }
      };
      
    } catch (error: any) {
      return {
        success: false,
        error: error.message,
        filepath: '',
        filename,
        context: 'server',
        timestamp
      };
    }
  }

  /**
   * Load html2canvas library dynamically
   */
  static async loadHtml2Canvas(
    html2canvasLoaded: boolean,
    html2canvasPromise: Promise<any> | null,
    setHtml2CanvasLoaded: (loaded: boolean) => void,
    setHtml2CanvasPromise: (promise: Promise<any> | null) => void,
    log: (component: string, message: string, data?: any) => void,
    critical: (component: string, message: string, data?: any) => void
  ): Promise<any> {
    
    if (typeof window === 'undefined') {
      throw new Error('html2canvas is only available in browser context');
    }

    // Check if already loaded
    if (html2canvasLoaded && (window as any).html2canvas) {
      return (window as any).html2canvas;
    }

    // Check if loading is in progress
    if (html2canvasPromise) {
      log('JTAG', 'html2canvas load already in progress, waiting...');
      return html2canvasPromise;
    }

    // Create new loading promise (only executed once)
    log('JTAG', 'Starting html2canvas dynamic load...');
    const loadPromise = new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
      
      script.onload = () => {
        log('JTAG', '✅ html2canvas loaded successfully');
        setHtml2CanvasLoaded(true);
        const html2canvas = (window as any).html2canvas;
        if (html2canvas) {
          resolve(html2canvas);
        } else {
          critical('JTAG_HTML2CANVAS', 'html2canvas not available after script load');
          reject(new Error('html2canvas not available after load'));
        }
      };
      
      script.onerror = (error) => {
        critical('JTAG_HTML2CANVAS', 'Failed to load html2canvas from CDN', { error });
        reject(new Error('Failed to load html2canvas library'));
      };
      
      document.head.appendChild(script);
      log('JTAG', 'html2canvas script injected, waiting for load...');
    });

    setHtml2CanvasPromise(loadPromise);
    return loadPromise;
  }
}