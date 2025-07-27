/**
 * FileLoad Command - Server Implementation
 * 
 * Server does file I/O work (its natural environment)
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import { PersistenceError } from '@shared/ErrorTypes';
import { type FileLoadParams, type FileLoadResult, createFileLoadResult } from '@fileLoadShared/FileLoadTypes';
import type { ICommandDaemon } from '@commandBase';

export class FileLoadServerCommand extends CommandBase<FileLoadParams, FileLoadResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-load', context, subpath, commander);
  }

  /**
   * Server does file operations natively (no delegation needed)
   */
  async execute(params: JTAGPayload): Promise<FileLoadResult> {
    const loadParams = params as FileLoadParams;
    
    console.log(`📖 SERVER: Loading file: ${loadParams.filepath}`);

    try {
      const resolvedPath = path.resolve(loadParams.filepath);
      
      // Check if file exists
      try {
        await fs.access(resolvedPath);
      } catch {
        throw new Error(`File not found: ${resolvedPath}`);
      }
      
      // Read file
      const content = await fs.readFile(resolvedPath, { encoding: (loadParams.encoding ?? 'utf8') as BufferEncoding });
      const stats = await fs.stat(resolvedPath);
      
      console.log(`✅ SERVER: Loaded ${stats.size} bytes from ${resolvedPath}`);
      
      return createFileLoadResult(params.context, params.sessionId, {
        success: true,
        filepath: resolvedPath,
        content: content,
        bytesRead: stats.size,
        exists: true
      });

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