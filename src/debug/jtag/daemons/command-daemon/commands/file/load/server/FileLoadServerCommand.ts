/**
 * FileLoad Command - Server Implementation
 * 
 * Server does file I/O work (its natural environment)
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../../../shared/JTAGTypes';
import type { FileLoadParams } from '../shared/FileLoadTypes';
import { FileLoadResult } from '../shared/FileLoadTypes';
import type { ICommandDaemon } from '../../../../shared/CommandBase';

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
      const content = await fs.readFile(resolvedPath, loadParams.encoding || 'utf8');
      const stats = await fs.stat(resolvedPath);
      
      console.log(`✅ SERVER: Loaded ${stats.size} bytes from ${resolvedPath}`);
      
      return new FileLoadResult({
        success: true,
        filepath: resolvedPath,
        content: content,
        bytesRead: stats.size,
        exists: true,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });

    } catch (error: any) {
      console.error(`❌ SERVER: File load failed:`, error.message);
      return new FileLoadResult({
        success: false,
        filepath: loadParams.filepath,
        content: '',
        bytesRead: 0,
        exists: false,
        error: error.message,
        environment: this.context.environment,
        timestamp: new Date().toISOString()
      });
    }
  }
}