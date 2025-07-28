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
import { type FileLoadParams, type FileLoadResult, createFileLoadResult } from '@commandsFileLoadShared/FileLoadTypes';
import type { ICommandDaemon } from '@commandBase';

export class FileLoadServerCommand extends CommandBase<FileLoadParams, FileLoadResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-load', context, subpath, commander);
  }

  /**
   * TEMPORARY: Session-based file loading until ArtifactoryDaemon is implemented
   */
  async execute(params: JTAGPayload): Promise<FileLoadResult> {
    const loadParams = params as FileLoadParams;
    
    console.log(`📖 SERVER: Loading file: ${loadParams.filepath}`);

    try {
      // TEMPORARY: Create session-based path manually
      const sessionId = loadParams.sessionId;
      const basePath = `.continuum/jtag/sessions/user/${sessionId}`;
      const fullPath = path.resolve(basePath, loadParams.filepath);
      
      console.log(`📝 SERVER: Loading from session path: ${fullPath}`);
      
      // Check if file exists
      try {
        await fs.access(fullPath);
      } catch {
        throw new Error(`File not found: ${fullPath}`);
      }
      
      // Read file
      const content = await fs.readFile(fullPath, { encoding: (loadParams.encoding ?? 'utf8') as BufferEncoding });
      const stats = await fs.stat(fullPath);
      
      console.log(`✅ SERVER: Loaded ${stats.size} bytes from ${fullPath}`);
      
      return createFileLoadResult(params.context, params.sessionId, {
        success: true,
        filepath: fullPath,
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