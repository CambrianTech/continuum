/**
 * FileLoad Command - Browser Implementation
 * 
 * Browser delegates to server for file I/O (can't read filesystem directly)
 */

import { type FileLoadParams, type FileLoadResult, createFileLoadResult } from '@fileLoadShared/FileLoadTypes';
import { FileLoadCommand } from '@fileLoadShared/FileLoadCommand';

export class FileLoadBrowserCommand extends FileLoadCommand {
  
  /**
   * Browser delegates file operations to server
   */
  async execute(params: FileLoadParams): Promise<FileLoadResult> {
    console.log(`📖 BROWSER: File load → delegating to server`);

    try {
      // Browser always delegates file I/O to server
      console.log(`🔀 BROWSER: Need filesystem access → delegating to server`);
      console.log(`📂 BROWSER: Loading "${params.filepath}"`);
      
      return await this.remoteExecute(params);

    } catch (error: any) {
      console.error(`❌ BROWSER: File load delegation failed:`, error.message);
      return createFileLoadResult(params.context, params.sessionId, {
        success: false,
        filepath: params.filepath,
        content: '',
        bytesRead: 0,
        exists: false,
        error: error.message
      });
    }
  }
}