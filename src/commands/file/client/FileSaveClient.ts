/**
 * FileSaveClient - Browser-side file saving functionality
 * 
 * Following middle-out architecture pattern:
 * - Handles browser-specific file operations
 * - Converts binary data to base64 for server transport
 * - Provides clean API for screenshot and other file saves
 */

import { ArtifactType } from '../../../types/shared/FileOperations';

export interface FileSaveClientOptions {
  content: string | Uint8Array;
  filename: string;
  artifactType?: ArtifactType | undefined;
}

export interface FileSaveClientResult {
  success: boolean;
  data?: {
    filename: string;
    filepath: string;
    size: number;
    artifactType: ArtifactType;
    sessionId: string;
    timestamp: string;
  };
  error?: string;
}

/**
 * Client-side file save implementation
 */
export class FileSaveClient {
  private static instance: FileSaveClient | null = null;

  /**
   * Get singleton instance
   */
  static getInstance(): FileSaveClient {
    FileSaveClient.instance ??= new FileSaveClient();
    return FileSaveClient.instance;
  }

  /**
   * Save file through the server command
   */
  async saveFile(options: FileSaveClientOptions): Promise<FileSaveClientResult> {
    try {
      const continuum = (window as any).continuum;
      if (!continuum) {
        throw new Error('Continuum not available');
      }

      console.log(`💾 FileSaveClient: Saving file ${options.filename}, size: ${options.content.length} bytes`);

      // Convert content to base64 for server transport
      const base64Content = await this.convertToBase64(options.content);

      // Execute the server-side write command
      const result = await continuum.execute('write', {
        filename: options.filename,
        content: base64Content,
        encoding: 'base64',
        artifactType: options.artifactType ?? ArtifactType.SCREENSHOT
      });

      return {
        success: true,
        data: result.data || result
      };

    } catch (error) {
      console.error('❌ FileSaveClient: Save failed:', error);
      return {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }

  /**
   * Convert content to base64 string
   */
  private async convertToBase64(content: string | Uint8Array): Promise<string> {
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

    throw new Error('Unsupported content type');
  }
}

/**
 * Convenience function for file saving
 */
export async function saveFile(options: FileSaveClientOptions): Promise<FileSaveClientResult> {
  const client = FileSaveClient.getInstance();
  return await client.saveFile(options);
}

// Make available globally for backward compatibility
// TODO: Remove in future versions
(window as any).FileSaveClient = FileSaveClient;
(window as any).saveFile = saveFile;