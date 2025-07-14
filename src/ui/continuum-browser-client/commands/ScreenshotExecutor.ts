// ISSUES: 1 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
// ✅ CROSS-CUTTING CONCERN FIXED: Browser response data structure now matches ScreenshotCommand expectations
/**
 * Screenshot Executor - Browser-side screenshot capture handler
 * 
 * ✅ MODULAR DESIGN: Self-contained screenshot execution module
 * ✅ HTML2CANVAS: Dynamic loading and screenshot capture
 * ✅ EVENT DRIVEN: Registers with WebSocket manager via events
 */

import { RemoteExecutionResponse } from '../types/WebSocketTypes';

export class ScreenshotExecutor {
  private static instance: ScreenshotExecutor | null = null;
  private isRegistered = false;

  /**
   * Singleton pattern for global event handler
   */
  static getInstance(): ScreenshotExecutor {
    if (!ScreenshotExecutor.instance) {
      ScreenshotExecutor.instance = new ScreenshotExecutor();
    }
    return ScreenshotExecutor.instance;
  }

  /**
   * Register screenshot handler with WebSocket manager
   */
  register(): void {
    if (this.isRegistered) return;

    document.addEventListener('continuum:remote_execution', this.handleRemoteExecution.bind(this) as EventListener);
    this.isRegistered = true;
    console.log('📸 ScreenshotExecutor registered for remote execution');
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
    console.log('📸 ScreenshotExecutor handling screenshot request', request);

    try {
      const result = await this.executeScreenshot(request.params);
      
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
      console.error('❌ Screenshot execution failed:', error);
      
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
   * Execute screenshot capture using html2canvas
   */
  private async executeScreenshot(params: Record<string, unknown>): Promise<{ imageData: string; format: string; selector: string; filename: string; width: number; height: number }> {
    console.log('📸 Executing screenshot with params:', params);

    // Load html2canvas dynamically if not already loaded
    if (typeof (window as any).html2canvas === 'undefined') {
      console.log('📦 Loading html2canvas library...');
      await this.loadHtml2Canvas();
    }

    const html2canvas = (window as any).html2canvas;
    const selector = (params.selector as string) || 'body';
    const format = (params.format as string) || 'png';
    const filename = (params.filename as string) || `screenshot-${Date.now()}.${format}`;

    // Find target element
    const element = document.querySelector(selector);
    if (!element) {
      throw new Error(`Screenshot target not found: ${selector}`);
    }

    console.log(`📸 Capturing screenshot of: ${selector}`);
    
    // Capture screenshot
    const canvas = await html2canvas(element as HTMLElement, {
      useCORS: true,
      allowTaint: true,
      scale: 1
    });

    // Convert to base64
    const imageData = canvas.toDataURL(`image/${format}`);
    
    console.log(`✅ Screenshot captured: ${imageData.length} bytes`);

    return {
      imageData,
      format,
      selector,
      filename,
      width: canvas.width,
      height: canvas.height
    };
  }

  /**
   * Dynamically load html2canvas library
   */
  private async loadHtml2Canvas(): Promise<void> {
    return new Promise((resolve, reject) => {
      const script = document.createElement('script');
      script.src = 'https://cdnjs.cloudflare.com/ajax/libs/html2canvas/1.4.1/html2canvas.min.js';
      script.onload = () => {
        console.log('✅ html2canvas loaded successfully');
        resolve();
      };
      script.onerror = () => {
        reject(new Error('Failed to load html2canvas library'));
      };
      document.head.appendChild(script);
    });
  }
}

// Auto-register when module loads
ScreenshotExecutor.getInstance().register();