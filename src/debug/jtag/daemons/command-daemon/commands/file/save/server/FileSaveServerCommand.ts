/**
 * FileSave Command - Server Implementation
 * 
 * CRITICAL: Commands NEVER bypass daemons for resource access
 * ALL file operations must go through ArtifactsDaemon
 */

import { CommandBase } from '@commandBase';
import type { JTAGContext, JTAGPayload } from '@shared/JTAGTypes';
import type { FileSaveParams } from '@fileSaveShared/FileSaveTypes';
import { FileSaveResult } from '@fileSaveShared/FileSaveTypes';
import type { ICommandDaemon } from '@commandBase';

export class FileSaveServerCommand extends CommandBase<FileSaveParams, FileSaveResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('file-save', context, subpath, commander);
  }

  /**
   * Server delegates ALL file operations to ArtifactsDaemon
   * NO DIRECT FS ACCESS - daemon provides smart file management
   */
  async execute(params: JTAGPayload): Promise<FileSaveResult> {
    const saveParams = params as FileSaveParams;
    
    console.log(`💾 SERVER: FileSave → delegating to ArtifactsDaemon`);

    try {
      // ALWAYS delegate to ArtifactsDaemon for intelligent file management:
      // - Permission checking
      // - File locking coordination  
      // - Concurrent access management
      // - Automatic file watching setup
      // - Backup/sync queue management
      // - Handle caching and cleanup
      
      console.log(`🔀 SERVER: Need file system access → delegating to ArtifactsDaemon`);
      console.log(`📝 SERVER: Saving ${saveParams.content.length} chars to "${saveParams.filepath}"`);
      
      // TODO: Implement ArtifactsDaemon.save() delegation
      // return await this.artifactsDaemon.save(saveParams);
      
      // TEMPORARY: Until ArtifactsDaemon is implemented
      throw new Error('ArtifactsDaemon not yet implemented - file commands require daemon delegation');

    } catch (error: any) {
      console.error(`❌ SERVER: FileSave delegation failed:`, error.message);
      return new FileSaveResult({
        success: false,
        filepath: saveParams.filepath,
        bytesWritten: 0,
        created: false,
        error: error.message,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });
    }
  }
}