/**
 * FileWriteCommand - Unified file writing with session management
 * 
 * Used by screenshot, artifactory, logging, session management, etc.
 * All file writes go through this command to ensure proper organization
 */

import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand } from '../base/BaseFileCommand';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface FileWriteParams {
  content: string | Buffer;
  filename: string;
  sessionId?: string;
  artifactType?: 'screenshot' | 'log' | 'recording' | 'file' | 'devtools' | 'metadata';
  directory?: string; // Override default location
  encoding?: BufferEncoding;
}

export class FileWriteCommand extends BaseFileCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'file_write',
      category: 'file',
      icon: '💾',
      description: 'Write files to session-managed locations using ContinuumDirectoryDaemon',
      parameters: {
        content: 'string|Buffer',
        filename: 'string',
        sessionId: 'string?',
        artifactType: 'string?',
        directory: 'string?',
        encoding: 'string?'
      },
      examples: [
        { description: 'Write log file', command: 'file_write --content="log data" --filename="app.log" --artifactType="log"' },
        { description: 'Save screenshot', command: 'file_write --content=<buffer> --filename="screenshot.png" --sessionId="session123" --artifactType="screenshot"' }
      ],
      usage: 'Write files to proper session locations with artifact organization'
    };
  }

  static async execute(params: FileWriteParams, _context?: CommandContext): Promise<CommandResult> {
    try {
      // 1. Get target directory from ContinuumDirectoryDaemon
      const targetPath = await this.getTargetPath(params);
      
      // 2. Ensure target directory exists
      await this.ensureDirectoryExists(path.dirname(targetPath));
      
      // 3. Write file with proper encoding
      const encoding = params.encoding || (Buffer.isBuffer(params.content) ? undefined : 'utf8');
      await fs.writeFile(targetPath, params.content, encoding ? { encoding } : undefined);
      
      // 4. Log the write operation
      await this.logFileOperation(targetPath, 'write');
      
      return this.createSuccessResult(
        `File written successfully: ${params.filename}`,
        {
          filename: params.filename,
          path: targetPath,
          size: Buffer.isBuffer(params.content) ? params.content.length : Buffer.byteLength(params.content, encoding),
          artifactType: params.artifactType,
          sessionId: params.sessionId,
          timestamp: new Date().toISOString()
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`File write failed: ${errorMessage}`);
    }
  }

  // All file path and directory methods inherited from BaseFileCommand
}