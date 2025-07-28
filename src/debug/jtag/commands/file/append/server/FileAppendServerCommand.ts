/**
 * FileAppend Command - Server Implementation
 * 
 * Server does file I/O work (its natural environment)
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import { PersistenceError } from '@shared/ErrorTypes';
import  { type FileAppendParams, type FileAppendResult, createFileAppendResult } from '@fileAppend/shared/FileAppendTypes';

export class FileAppendServerCommand extends CommandBase<FileAppendParams, FileAppendResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-append', context, subpath, commander);
  }

  /**
   * TEMPORARY: Session-based file append until ArtifactoryDaemon is implemented
   */
  async execute(params: JTAGPayload): Promise<FileAppendResult> {
    const appendParams = params as FileAppendParams;
    
    console.log(`📝 SERVER: Appending to file: ${appendParams.filepath}`);

    try {
      // TEMPORARY: Create session-based path manually
      const sessionId = appendParams.sessionId;
      const basePath = `.continuum/jtag/sessions/user/${sessionId}`;
      const fullPath = path.resolve(basePath, appendParams.filepath);
      let wasCreated = false;
      
      console.log(`📝 SERVER: Appending to session path: ${fullPath}`);
      
      // Check if file exists
      try {
        await fs.access(fullPath);
      } catch {
        if (appendParams.createIfMissing) {
          // Create parent directories if needed
          await fs.mkdir(path.dirname(fullPath), { recursive: true });
          wasCreated = true;
        } else {
          throw new Error(`File not found: ${fullPath}`);
        }
      }
      
      // Append content
      await fs.appendFile(fullPath, appendParams.content, { encoding: (appendParams.encoding ?? 'utf8') as BufferEncoding });

      const bytesAppended = Buffer.byteLength(appendParams.content, (appendParams.encoding ?? 'utf8') as BufferEncoding);
      
      console.log(`✅ SERVER: Appended ${bytesAppended} bytes to ${fullPath}`);
      
      return createFileAppendResult(params.context, params.sessionId, {
        success: true,
        filepath: fullPath,
        exists: true,
        bytesAppended: bytesAppended,
        wasCreated: wasCreated
      });

    } catch (error: any) {
      console.error(`❌ SERVER: File append failed:`, error.message);
      const appendError = error instanceof Error ? new PersistenceError(appendParams.filepath, 'write', error.message, { cause: error }) : new PersistenceError(appendParams.filepath, 'write', String(error));
      return createFileAppendResult(params.context, params.sessionId, {
        success: false,
        filepath: appendParams.filepath,
        exists: false,
        bytesAppended: 0,
        wasCreated: false,
        error: appendError
      });
    }
  }
}