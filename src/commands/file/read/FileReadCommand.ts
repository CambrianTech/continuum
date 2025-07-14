// ISSUES: 0 open, last updated 2025-07-13 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileReadCommand - Read files with session management (like cat)
 * 
 * ✅ FIXED: CLI parameter parsing with --file alias support
 * ✅ ARCHITECTURE: Parameters pre-parsed by UniversalCommandRegistry
 * 🔬 MIDDLE-OUT: Layer 3 Command System with centralized parsing
 * 
 * Used throughout the system for reading logs, configs, artifacts, etc.
 */

import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';
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
        filename: { type: 'string' as const, description: 'Name of the file to read' },
        sessionId: { type: 'string' as const, description: 'Session ID for file organization', required: false },
        artifactType: { type: 'string' as const, description: 'Type of artifact (screenshot|log|recording|file|devtools|metadata)', required: false },
        directory: { type: 'string' as const, description: 'Directory override for file location', required: false },
        encoding: { type: 'string' as const, description: 'Text encoding for the file', required: false },
        lines: { type: 'number' as const, description: 'Limit number of lines to read', required: false },
        offset: { type: 'number' as const, description: 'Start reading from line offset', required: false }
      },
      examples: [
        { description: 'Read log file', command: 'file_read --filename="app.log" --artifactType="log"' },
        { description: 'Read session metadata', command: 'file_read --filename="session.json" --sessionId="session123"' },
        { description: 'Read last 10 lines', command: 'file_read --filename="debug.log" --lines=10' }
      ],
      usage: 'Read files from session-managed locations'
    };
  }

  static async execute(params: FileReadParams, _context?: ContinuumContext): Promise<CommandResult> {
    try {
      // Handle CLI aliases: --file should map to filename (parameters already parsed by registry)
      const filename = params.filename || (params as any).file;
      
      if (!filename) {
        return this.createErrorResult('Missing required parameter: filename (use --filename or --file)');
      }
      
      // 1. Get file path using session management
      const filePath = await this.getTargetPath({
        filename: filename,
        sessionId: params.sessionId,
        artifactType: params.artifactType,
        directory: params.directory
      });
      
      // 2. Check if file exists
      if (!(await this.fileExists(filePath))) {
        return this.createErrorResult(`File not found: ${filename}`);
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