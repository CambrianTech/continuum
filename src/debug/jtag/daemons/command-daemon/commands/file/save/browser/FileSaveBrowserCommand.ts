/**
 * FileSave Command - Browser Implementation
 * 
 * Browser delegates to server for file I/O (can't write to filesystem directly)
 */

import { type FileSaveParams, FileSaveResult } from '@fileSaveShared/FileSaveTypes';
import { FileSaveCommand } from '@fileSaveShared/FileSaveCommand';

export class FileSaveBrowserCommand extends FileSaveCommand {
  
  /**
   * Browser delegates file operations to server
   */
  async execute(params: FileSaveParams): Promise<FileSaveResult> {
    console.log(`💾 BROWSER: File save → delegating to server`);

    try {
      // Browser always delegates file I/O to server
      console.log(`🔀 BROWSER: Need filesystem access → delegating to server`);
      console.log(`📝 BROWSER: Saving ${params.content.length} chars to "${params.filepath}"`);
      
      return await this.remoteExecute(params);

    } catch (error: any) {
      console.error(`❌ BROWSER: File save delegation failed:`, error.message);
      return new FileSaveResult({
        success: false,
        filepath: params.filepath,
        bytesWritten: 0,
        created: false,
        error: error.message,
        timestamp: new Date().toISOString()
      }, params.context, params.sessionId);
    }
  }
}