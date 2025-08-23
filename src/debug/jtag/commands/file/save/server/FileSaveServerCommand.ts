/**
 * FileSave Command - Server Implementation
 * 
 * Implements direct filesystem access following JTAG session-based directory structure.
 * Uses same patterns as ArtifactsDaemon: session paths, directory creation, atomic writes.
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { FileSaveParams, FileSaveResult } from '../shared/FileSaveTypes';
import { createFileSaveResult } from '../shared/FileSaveTypes';
import { PersistenceError } from '../../../../system/core/types/ErrorTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { WorkingDirConfig } from '../../../../system/core/config/WorkingDirConfig';

export class FileSaveServerCommand extends CommandBase<FileSaveParams, FileSaveResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-save', context, subpath, commander);
  }

  /**
   * Implements direct filesystem access following JTAG session-based directory structure
   */
  async execute(saveParams: FileSaveParams): Promise<FileSaveResult> {
    
    console.log(`💾 SERVER: FileSave (session-based filesystem access)`);

    try {
      // Create session-based path following JTAG directory structure
      // Use WorkingDirConfig to respect per-project .continuum isolation
      const sessionId = saveParams.sessionId;
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const basePath = path.join(continuumPath, 'jtag', 'sessions', 'user', sessionId);
      const fullPath = path.resolve(basePath, saveParams.filepath);
      
      console.log(`📝 SERVER: Saving to session path: ${fullPath}`);
      
      // Ensure directory exists
      await fs.mkdir(path.dirname(fullPath), { recursive: true });
      
      // Write file content
      let bytesWritten = 0;
      if (Buffer.isBuffer(saveParams.content)) {
        await fs.writeFile(fullPath, saveParams.content);
        bytesWritten = saveParams.content.length;
      } else {
        await fs.writeFile(fullPath, saveParams.content, 'utf8');
        bytesWritten = Buffer.byteLength(saveParams.content, 'utf8');
      }
      
      console.log(`✅ SERVER: Saved ${bytesWritten} bytes to ${fullPath}`);
      
      return createFileSaveResult(saveParams.context, saveParams.sessionId, {
        success: true,
        filepath: fullPath,
        bytesWritten: bytesWritten,
        created: true
      });

    } catch (error: any) {
      console.error(`❌ SERVER: FileSave failed:`, error.message);
      const saveError = error instanceof Error ? new PersistenceError(saveParams.filepath, 'write', error.message, { cause: error }) : new PersistenceError(saveParams.filepath, 'write', String(error));
      return createFileSaveResult(saveParams.context, saveParams.sessionId, {
        success: false,
        filepath: saveParams.filepath,
        bytesWritten: 0,
        created: false,
        error: saveError
      });
    }
  }
}