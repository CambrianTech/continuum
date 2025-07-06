/**
 * FileAppendCommand - Append to files with session management
 * 
 * Used for logs, continuous data collection, etc.
 */

import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand } from '../base/BaseFileCommand';
import * as fs from 'fs/promises';
import * as path from 'path';

export interface FileAppendParams {
  content: string;
  filename: string;
  sessionId?: string;
  artifactType?: 'screenshot' | 'log' | 'recording' | 'file' | 'devtools' | 'metadata';
  directory?: string;
  encoding?: BufferEncoding;
  newline?: boolean; // Add newline before content (default: true)
}

export class FileAppendCommand extends BaseFileCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'file_append',
      category: 'file',
      icon: '📝',
      description: 'Append content to files with session management',
      parameters: {
        content: { type: 'string' as const, description: 'Content to append to the file' },
        filename: { type: 'string' as const, description: 'Name of the file to append to' },
        sessionId: { type: 'string' as const, description: 'Session ID for file organization', required: false },
        artifactType: { type: 'string' as const, description: 'Type of artifact (screenshot|log|recording|file|devtools|metadata)', required: false },
        directory: { type: 'string' as const, description: 'Directory override for file location', required: false },
        encoding: { type: 'string' as const, description: 'Text encoding for the file', required: false },
        newline: { type: 'boolean' as const, description: 'Add newline before content', required: false }
      },
      examples: [
        { description: 'Append to log', command: 'file_append --content="ERROR: Connection failed" --filename="app.log" --artifactType="log"' },
        { description: 'Append to session log', command: 'file_append --content="User action" --filename="activity.log" --sessionId="session123"' }
      ],
      usage: 'Append content to files in session-managed locations'
    };
  }

  static async execute(params: FileAppendParams, _context?: CommandContext): Promise<CommandResult> {
    try {
      // 1. Get target path using session management
      const targetPath = await this.getTargetPath({
        filename: params.filename,
        sessionId: params.sessionId,
        artifactType: params.artifactType,
        directory: params.directory
      });
      
      // 2. Ensure target directory exists
      await this.ensureDirectoryExists(path.dirname(targetPath));
      
      // 3. Prepare content with optional newline
      const newline = params.newline !== false; // Default to true
      const contentToAppend = newline && params.content ? '\n' + params.content : params.content;
      
      // 4. Append to file
      const encoding = params.encoding || 'utf8';
      await fs.appendFile(targetPath, contentToAppend, { encoding });
      
      // 5. Log the append operation
      await this.logFileOperation('file_append', targetPath, {
        filename: params.filename,
        sessionId: params.sessionId,
        artifactType: params.artifactType,
        contentLength: params.content.length,
        newline
      });
      
      // 6. Get updated file stats
      const stats = await this.getFileStats(targetPath);
      
      return this.createSuccessResult(
        `Content appended successfully to: ${params.filename}`,
        {
          filename: params.filename,
          path: targetPath,
          appendedLength: contentToAppend.length,
          totalSize: stats?.size || 0,
          modified: stats?.mtime,
          sessionId: params.sessionId,
          artifactType: params.artifactType
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`File append failed: ${errorMessage}`);
    }
  }
}