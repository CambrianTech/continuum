/**
 * FileAppend Command - Server Implementation
 * 
 * Server does file I/O work (its natural environment)
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../../shared/JTAGTypes';
import type { FileAppendParams } from '../shared/FileAppendTypes';
import { FileAppendResult } from '../shared/FileAppendTypes';
import type { ICommandDaemon } from '../../../../shared/CommandBase';

export class FileAppendServerCommand extends CommandBase<FileAppendParams, FileAppendResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-append', context, subpath, commander);
  }

  /**
   * Server does file operations natively (no delegation needed)
   */
  async execute(params: JTAGPayload): Promise<FileAppendResult> {
    const appendParams = params as FileAppendParams;
    
    console.log(`📝 SERVER: Appending to file: ${appendParams.filepath}`);

    try {
      const resolvedPath = path.resolve(appendParams.filepath);
      let wasCreated = false;
      
      // Check if file exists
      try {
        await fs.access(resolvedPath);
      } catch {
        if (appendParams.createIfMissing) {
          // Create parent directories if needed
          await fs.mkdir(path.dirname(resolvedPath), { recursive: true });
          wasCreated = true;
        } else {
          throw new Error(`File not found: ${resolvedPath}`);
        }
      }
      
      // Append content
      await fs.appendFile(resolvedPath, appendParams.content, appendParams.encoding || 'utf8');
      
      const stats = await fs.stat(resolvedPath);
      const bytesAppended = Buffer.byteLength(appendParams.content, appendParams.encoding || 'utf8');
      
      console.log(`✅ SERVER: Appended ${bytesAppended} bytes to ${resolvedPath}`);
      
      return new FileAppendResult({
        success: true,
        filepath: resolvedPath,
        exists: true,
        bytesAppended: bytesAppended,
        wasCreated: wasCreated,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });

    } catch (error: any) {
      console.error(`❌ SERVER: File append failed:`, error.message);
      return new FileAppendResult({
        success: false,
        filepath: appendParams.filepath,
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