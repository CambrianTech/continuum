// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileSaveClient - Browser-side file saving functionality
 * 
 * Following middle-out architecture pattern:
 * - Handles browser-specific file operations
 * - Converts binary data to base64 for server transport
 * - Provides clean API for screenshot and other file saves
 */

import { FileClient } from '../../client/FileClient';
import { ArtifactType, FileSaveClientOptions, FileSaveClientResult } from '../../shared/FileTypes';

/**
 * Client-side file save implementation extending FileClient
 */
export class FileSaveClient extends FileClient {
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
      // 1. Check browser support
      const browserSupport = FileSaveClient.checkBrowserSupport();
      if (!browserSupport.supported) {
        throw new Error(`Browser missing required APIs: ${browserSupport.missing.join(', ')}`);
      }

      // 2. Log client operation
      FileSaveClient.logClientOperation('saveFile', options);

      // 3. Get continuum instance
      const continuum = FileSaveClient.getContinuumInstance();

      // 4. Convert content to base64 for server transport
      const base64Content = await FileSaveClient.convertToBase64(options.content);

      // 5. Basic validation
      if (!base64Content) {
        throw new Error('Base64 content conversion failed');
      }

      // 6. Execute the server-side save command
      const result = await continuum.execute('file_save', {
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
   * Save file with preview generation
   */
  async saveFileWithPreview(options: FileSaveClientOptions): Promise<FileSaveClientResult & { preview?: string }> {
    try {
      // Save the file
      const result = await this.saveFile(options);

      // Generate preview if it's an image
      if (result.success && result.data) {
        const mimeType = FileSaveClient.detectMimeType(options.filename);
        if (mimeType.startsWith('image/')) {
          const preview = FileSaveClient.createDataUrl(options.content, mimeType);
          return { ...result, preview };
        }
      }

      return result;
    } catch (error) {
      FileSaveClient.handleClientError(error, 'saveFileWithPreview');
    }
  }
}

/**
 * Convenience function for file saving
 */
export async function saveFile(options: FileSaveClientOptions): Promise<FileSaveClientResult> {
  const client = FileSaveClient.getInstance();
  return await client.saveFile(options);
}

/**
 * Convenience function for file saving with preview
 */
export async function saveFileWithPreview(options: FileSaveClientOptions): Promise<FileSaveClientResult & { preview?: string }> {
  const client = FileSaveClient.getInstance();
  return await client.saveFileWithPreview(options);
}

// Make available globally for backward compatibility
// TODO: Remove in future versions
(window as any).FileSaveClient = FileSaveClient;
(window as any).saveFile = saveFile;
(window as any).saveFileWithPreview = saveFileWithPreview;