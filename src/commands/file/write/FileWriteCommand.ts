/**
 * FileWriteCommand - Unified file writing with session management
 * 
 * Used by screenshot, artifactory, logging, session management, etc.
 * All file writes go through this command to ensure proper organization
 */

import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand, FileSystemOperation } from '../base/BaseFileCommand';
// Removed direct fs import - now delegates to ContinuumFileSystemDaemon via BaseFileCommand
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
        content: { type: 'string' as const, description: 'Content to write to the file (string or Buffer)' },
        filename: { type: 'string' as const, description: 'Name of the file to write' },
        sessionId: { type: 'string' as const, description: 'Session ID for file organization', required: false },
        artifactType: { type: 'string' as const, description: 'Type of artifact (screenshot|log|recording|file|devtools|metadata)', required: false },
        directory: { type: 'string' as const, description: 'Directory override for file location', required: false },
        encoding: { type: 'string' as const, description: 'Text encoding for the file', required: false }
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
      const targetPath = await this.getTargetPath({
        filename: params.filename,
        sessionId: params.sessionId,
        artifactType: params.artifactType,
        directory: params.directory
      });
      
      // 2. Ensure target directory exists
      await this.ensureDirectoryExists(path.dirname(targetPath));
      
      // 3. Write file using ContinuumFileSystemDaemon delegation
      const encoding = params.encoding || (Buffer.isBuffer(params.content) ? undefined : 'utf8');
      await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.WRITE_FILE, {
        path: targetPath,
        content: params.content,
        encoding: encoding
      });
      
      // 4. Log the write operation via daemon delegation
      await this.logFileOperation('write', targetPath, {
        artifactType: params.artifactType,
        sessionId: params.sessionId,
        size: Buffer.isBuffer(params.content) ? params.content.length : Buffer.byteLength(params.content, encoding || 'utf8')
      });
      
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