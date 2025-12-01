/**
 * FileLoad Command - Server Implementation
 * 
 * Server does file I/O work (its natural environment)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { PersistenceError } from '../../../../system/core/types/ErrorTypes';
import { type FileLoadParams, type FileLoadResult, createFileLoadResult } from '../shared/FileLoadTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { FileDaemon } from '../../../../daemons/file-daemon/shared/FileDaemon';

export class FileLoadServerCommand extends CommandBase<FileLoadParams, FileLoadResult> {
  private fileDaemon: FileDaemon;
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-load', context, subpath, commander);
    this.fileDaemon = new FileDaemon();
  }

  /**
   * Load file using FileDaemon with proper path resolution
   */
  async execute(params: JTAGPayload): Promise<FileLoadResult> {
    const loadParams = params as FileLoadParams;

    // console.log(`📖 SERVER: Loading file: ${loadParams.filepath}`);

    try {
      const result = await this.fileDaemon.loadFile(
        loadParams.filepath,
        loadParams.sessionId,
        (loadParams.encoding ?? 'utf8') as BufferEncoding
      );
      
      if (result.success) {
        return createFileLoadResult(params.context, params.sessionId, {
          success: true,
          filepath: result.resolvedPath,
          content: result.content,
          bytesRead: result.bytesRead,
          exists: result.exists
        });
      } else {
        const loadError = result.error ? new PersistenceError(loadParams.filepath, 'read', result.error.message, { cause: result.error }) : new PersistenceError(loadParams.filepath, 'read', 'Unknown error');
        return createFileLoadResult(params.context, params.sessionId, {
          success: false,
          filepath: loadParams.filepath,
          content: '',
          bytesRead: 0,
          exists: false,
          error: loadError
        });
      }

    } catch (error: any) {
      console.error(`❌ SERVER: File load failed:`, error.message);
      const loadError = error instanceof Error ? new PersistenceError(loadParams.filepath, 'read', error.message, { cause: error }) : new PersistenceError(loadParams.filepath, 'read', String(error));
      return createFileLoadResult(params.context, params.sessionId, {
        success: false,
        filepath: loadParams.filepath,
        content: '',
        bytesRead: 0,
        exists: false,
        error: loadError
      });
    }
  }
}