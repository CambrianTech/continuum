/**
 * FileReadCommand - Read files with session management (like cat)
 * 
 * Used throughout the system for reading logs, configs, artifacts, etc.
 */

import { CommandDefinition, CommandContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand } from '../base/BaseFileCommand';
import * as fs from 'fs/promises';

export interface FileReadParams {
  filename: string;
  sessionId?: string;
  artifactType?: 'screenshot' | 'log' | 'recording' | 'file' | 'devtools' | 'metadata';
  directory?: string;
  encoding?: BufferEncoding;
  lines?: number; // Limit number of lines (like head/tail)
  offset?: number; // Start from line offset
}

export class FileReadCommand extends BaseFileCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'file_read',
      category: 'file',
      icon: '📖',
      description: 'Read files with session management - equivalent to cat',
      parameters: {
        filename: 'string',
        sessionId: 'string?',
        artifactType: 'string?',
        directory: 'string?',
        encoding: 'string?',
        lines: 'number?',
        offset: 'number?'
      },
      examples: [
        { description: 'Read log file', command: 'file_read --filename="app.log" --artifactType="log"' },
        { description: 'Read session metadata', command: 'file_read --filename="session.json" --sessionId="session123"' },
        { description: 'Read last 10 lines', command: 'file_read --filename="debug.log" --lines=10' }
      ],
      usage: 'Read files from session-managed locations'
    };
  }

  static async execute(params: FileReadParams, context?: CommandContext): Promise<CommandResult> {
    try {
      // 1. Get file path using session management
      const filePath = await this.getTargetPath({
        filename: params.filename,
        sessionId: params.sessionId,
        artifactType: params.artifactType,
        directory: params.directory
      });
      
      // 2. Check if file exists
      if (!(await this.fileExists(filePath))) {
        return this.createErrorResult(`File not found: ${params.filename}`);
      }
      
      // 3. Read file with proper encoding
      const encoding = params.encoding || 'utf8';
      let content = await fs.readFile(filePath, { encoding });
      
      // 4. Apply line limits if specified
      if (params.lines || params.offset) {
        const lines = content.split('\n');
        const start = params.offset || 0;
        const end = params.lines ? start + params.lines : lines.length;
        content = lines.slice(start, end).join('\n');
      }
      
      // 5. Log the read operation
      await this.logFileOperation('file_read', filePath, {
        filename: params.filename,
        sessionId: params.sessionId,
        artifactType: params.artifactType,
        lines: params.lines,
        offset: params.offset
      });
      
      // 6. Get file stats
      const stats = await this.getFileStats(filePath);
      
      return this.createSuccessResult(
        `File read successfully: ${params.filename}`,
        {
          filename: params.filename,
          path: filePath,
          content,
          encoding,
          size: stats?.size || 0,
          modified: stats?.mtime,
          lines: content.split('\n').length,
          sessionId: params.sessionId,
          artifactType: params.artifactType
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`File read failed: ${errorMessage}`);
    }
  }
}