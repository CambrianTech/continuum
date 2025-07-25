/**
 * FileAppend Command - Browser Implementation
 * 
 * Browser delegates to server for file I/O (can't write to filesystem directly)
 */

import { FileAppendParams, FileAppendResult } from '../shared/FileAppendTypes';
import { FileAppendCommand } from '../shared/FileAppendCommand';

export class FileAppendBrowserCommand extends FileAppendCommand {
  
  /**
   * Browser delegates file operations to server
   */
  async execute(params: FileAppendParams): Promise<FileAppendResult> {
    console.log(`📝 BROWSER: File append → delegating to server`);

    try {
      // Browser always delegates file I/O to server
      console.log(`🔀 BROWSER: Need filesystem access → delegating to server`);
      console.log(`➕ BROWSER: Appending ${params.content.length} chars to "${params.filepath}"`);
      
      return await this.remoteExecute(params);

    } catch (error: any) {
      console.error(`❌ BROWSER: File append delegation failed:`, error.message);
      return new FileAppendResult({
        success: false,
        filepath: params.filepath,
        exists: false,
        bytesAppended: 0,
        wasCreated: false,
        error: error.message,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });
    }
  }
}