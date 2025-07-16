// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileClient - Base client class for browser-side file operations
 * 
 * Following middle-out architecture pattern:
 * - Client-specific functionality for browser context
 * - Extends shared BaseFileCommand for common operations
 * - Handles browser-specific APIs and constraints
 */

import { BaseFileCommand } from '../shared/BaseFileCommand';
import { FileOperationParams, FileOperationResult, FileValidationResult } from '../shared/FileTypes';
import { FileValidator } from '../shared/FileValidator';

/**
 * Browser-specific file operation result
 */
export interface BrowserFileResult extends FileOperationResult {
  dataUrl?: string; // For preview purposes
  blob?: Blob; // Browser-specific blob data
}

/**
 * Base client class for browser-side file operations
 */
export abstract class FileClient extends BaseFileCommand {
  
  /**
   * Get continuum global instance
   */
  protected static getContinuumInstance(): any {
    const continuum = (window as any).continuum;
    if (!continuum) {
      throw new Error('Continuum not available in browser context');
    }
    return continuum;
  }

  /**
   * Validate file operation parameters using shared validator
   */
  protected static validateParams(params: FileOperationParams): FileValidationResult {
    return FileValidator.validateParams(params);
  }

  /**
   * Send file operation to server via continuum.execute()
   */
  protected static async sendToServer(command: string, params: Record<string, any>): Promise<any> {
    const continuum = this.getContinuumInstance();
    return await continuum.execute(command, params);
  }

  /**
   * Convert content to base64 for server transport
   */
  protected static async convertToBase64(content: string | Uint8Array): Promise<string> {
    if (typeof content === 'string') {
      return btoa(content);
    }

    // Handle Uint8Array using FileReader for large files
    if (content instanceof Uint8Array) {
      const blob = new Blob([content]);
      const reader = new FileReader();
      
      return new Promise<string>((resolve, reject) => {
        reader.onload = (): void => {
          const result = reader.result as string;
          // Remove the data URL prefix to get just the base64 content
          const base64Data = result.split(',')[1];
          resolve(base64Data);
        };
        reader.onerror = (): void => reject(reader.error);
        reader.readAsDataURL(blob);
      });
    }

    throw new Error('Unsupported content type for base64 conversion');
  }

  /**
   * Convert base64 to Uint8Array for processing
   */
  protected static base64ToUint8Array(base64: string): Uint8Array {
    const binaryString = atob(base64);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }
    return bytes;
  }

  /**
   * Create data URL for preview purposes
   */
  protected static createDataUrl(content: string | Uint8Array, mimeType: string): string {
    if (typeof content === 'string') {
      return `data:${mimeType};base64,${btoa(content)}`;
    }
    
    if (content instanceof Uint8Array) {
      const blob = new Blob([content], { type: mimeType });
      return URL.createObjectURL(blob);
    }
    
    throw new Error('Unsupported content type for data URL creation');
  }

  /**
   * Detect MIME type from filename
   */
  protected static detectMimeType(filename: string): string {
    const ext = filename.split('.').pop()?.toLowerCase();
    
    switch (ext) {
      case 'png': return 'image/png';
      case 'jpg':
      case 'jpeg': return 'image/jpeg';
      case 'gif': return 'image/gif';
      case 'svg': return 'image/svg+xml';
      case 'webp': return 'image/webp';
      case 'pdf': return 'application/pdf';
      case 'txt': return 'text/plain';
      case 'html': return 'text/html';
      case 'css': return 'text/css';
      case 'js': return 'application/javascript';
      case 'json': return 'application/json';
      default: return 'application/octet-stream';
    }
  }

  /**
   * Log client-side file operation
   */
  protected static logClientOperation(operation: string, params: any): void {
    console.log(`💾 FileClient: ${operation}`, {
      filename: params.filename,
      size: params.content?.length || 0,
      artifactType: params.artifactType,
      timestamp: new Date().toISOString()
    });
  }

  /**
   * Handle client-side errors consistently
   */
  protected static handleClientError(error: any, operation: string): never {
    console.error(`❌ FileClient: ${operation} failed:`, error);
    throw new Error(`File ${operation} failed: ${error instanceof Error ? error.message : String(error)}`);
  }

  /**
   * Check if browser supports required APIs
   */
  protected static checkBrowserSupport(): { supported: boolean; missing: string[] } {
    const missing: string[] = [];
    
    if (!window.FileReader) missing.push('FileReader');
    if (!window.Blob) missing.push('Blob');
    if (!window.URL) missing.push('URL');
    if (!window.atob) missing.push('atob');
    if (!window.btoa) missing.push('btoa');
    
    return {
      supported: missing.length === 0,
      missing
    };
  }
}