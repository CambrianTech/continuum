/**
 * FileSave Command - Server Implementation
 * 
 * TEMPORARY: Direct filesystem access until ArtifactsDaemon is implemented
 * TODO: Replace with proper ArtifactsDaemon delegation
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import type { FileSaveParams, FileSaveResult } from '../shared/FileSaveTypes';
import { createFileSaveResult } from '../shared/FileSaveTypes';
import { PersistenceError } from '../../../../system/core/types/ErrorTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';

export class FileSaveServerCommand extends CommandBase<FileSaveParams, FileSaveResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-save', context, subpath, commander);
  }

  /**
   * TEMPORARY: Direct filesystem access until ArtifactsDaemon is implemented
   * Creates session-based directory structure manually
   */
  async execute(params: JTAGPayload): Promise<FileSaveResult> {
    const saveParams = params as FileSaveParams;
    
    console.log(`💾 SERVER: FileSave (TEMPORARY direct FS access)`);
    
    // MANUAL DEBUG LOG - write directly to filesystem to confirm execution
    await fs.appendFile('/tmp/debug-filesave.log', `FileSave execute called at ${new Date().toISOString()} for ${saveParams.filepath}\n`).catch(() => {});

    try {
      // TEMPORARY: Create session-based path manually
      const sessionId = saveParams.sessionId;
      const basePath = `.continuum/jtag/sessions/user/${sessionId}`;
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