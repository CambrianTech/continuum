/**
 * FileSave Command - Browser Implementation
 * 
 * Browser delegates to server for file I/O (can't write to filesystem directly)
 */

import { type FileSaveParams, type FileSaveResult, createFileSaveResult } from '../shared/FileSaveTypes';
import { NetworkError } from '../../../../system/core/types/ErrorTypes';
import { FileSaveCommand } from '../shared/FileSaveCommand';

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
      const saveError = error instanceof Error ? new NetworkError('server', error.message, { cause: error }) : new NetworkError('server', String(error));
      return createFileSaveResult(params.context, params.sessionId, {
        success: false,
        filepath: params.filepath,
        bytesWritten: 0,
        created: false,
        error: saveError
      });
    }
  }
}