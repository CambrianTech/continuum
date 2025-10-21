/**
 * CLI formatter for screenshot command results
 */
import { BaseFormatter } from './BaseFormatter';

export class ScreenshotFormatter extends BaseFormatter {
  /**
   * Check if this formatter can handle the given result
   */
  canHandle(result: any, command: string): boolean {
    // Handle both direct screenshot results and broadcast responses
    return command === 'screenshot' && 
           result.success && 
           result.data && 
           (result.data.imageData || result.data.broadcastSent);
  }

  /**
   * Format screenshot result for user-friendly CLI output
   */
  format(result: any): void {
    const data = result.data;
    const status = this.getStatusDisplay(result.success);
    
    console.log(`\n📸 SCREENSHOT CAPTURED`);
    console.log(`${status.icon} Status: ${status.text}`);
    
    if (result.success && data) {
      // Handle broadcast response (current format)
      if (data.broadcastSent) {
        console.log(`📡 Broadcast sent to ${data.connectionCount} browser connection(s)`);
        console.log(`💾 Screenshot saved to session directory`);
        console.log(`📁 Check your screenshots folder for the captured image`);
      } else {
        // Handle direct response (future format)
        this.formatBasicInfo(data);
        this.formatPathInfo(data);
        this.formatDimensions(data);
        this.formatAIMetadata(data);
      }
    }
    
    if (result.executionTime) {
      console.log(`⏱️ Execution time: ${this.formatExecutionTime(result.executionTime)}`);
    }
    
    this.formatAIHint();
  }

  /**
   * Format basic file information
   */
  private formatBasicInfo(data: any): void {
    console.log(`📁 File: ${data.filename}`);
    
    if (data.fileSizeBytes) {
      console.log(`💾 File size: ${this.formatFileSize(data.fileSizeBytes)}`);
    }
  }

  /**
   * Format path information
   */
  private formatPathInfo(data: any): void {
    if (data.filePath) {
      console.log(`💾 Saved to: ${data.filePath}`);
    }
    
    if (data.fullPath) {
      console.log(`🔗 Full path: ${data.fullPath}`);
    }
  }

  /**
   * Format element and dimension information
   */
  private formatDimensions(data: any): void {
    if (data.elementName) {
      console.log(`🎯 Element: ${data.elementName} (${data.selector})`);
    } else {
      console.log(`🎯 Target: ${data.selector}`);
    }
    
    console.log(`📏 Dimensions: ${data.width}x${data.height}px`);
  }

  /**
   * Format AI-friendly metadata
   */
  private formatAIMetadata(data: any): void {
    if (data.originalWidth && data.originalHeight) {
      console.log(`🔍 Original: ${data.originalWidth}x${data.originalHeight}px`);
    }
    
    if (data.scale && data.scale !== 1) {
      console.log(`⚡ Scale: ${data.scale}x`);
    }
    
    if (data.cropped) {
      console.log(`✂️ Cropped: Yes`);
    }
    
    if (data.compressed) {
      console.log(`🗜️ Compressed: Yes`);
    }
    
    if (data.destination === 'bytes' || data.destination === 'both') {
      console.log(`🔢 Bytes: Available for AI processing`);
    }
  }

  /**
   * Format AI-friendly hint
   */
  private formatAIHint(): void {
    console.log(`\n🤖 AI-Friendly: Full JSON data available programmatically`);
    console.log(`📋 For complete data: Add --json flag or use programmatic access\n`);
  }
}