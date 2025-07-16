// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileSaveCommand - Save binary data (like screenshots) to files
 * 
 * Handles base64 encoded content and binary data, saves to the screenshots 
 * directory for the current session. Built on the unified file system architecture.
 */

import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand, FileSystemOperation } from '../shared/BaseFileCommand';
import { ArtifactType } from '../shared/FileTypes';
import * as path from 'path';

export interface FileSaveParams {
  content: string | Buffer;
  filename: string;
  sessionId?: string;
  encoding?: 'base64' | 'binary' | 'utf8';
  artifactType?: ArtifactType;
  directory?: string; // Override default location
  marshalId?: string; // For command chaining correlation
}

export class FileSaveCommand extends BaseFileCommand {
  static getDefinition(): CommandDefinition {
    return {
      name: 'file_save',
      category: 'file',
      icon: '💾',
      description: 'Save binary data (like screenshots) to files with base64 support',
      parameters: {
        content: { type: 'string' as const, description: 'Content to save (string, Buffer, or base64 encoded)' },
        filename: { type: 'string' as const, description: 'Name of the file to save' },
        sessionId: { type: 'string' as const, description: 'Session ID for file organization', required: false },
        encoding: { type: 'string' as const, description: 'Content encoding (base64|binary|utf8)', required: false },
        artifactType: { type: 'string' as const, description: 'Type of artifact (screenshot|recording|file|devtools|metadata)', required: false },
        directory: { type: 'string' as const, description: 'Directory override for file location', required: false }
      },
      examples: [
        { description: 'Save base64 screenshot', command: 'file_save --content="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==" --filename="screenshot.png" --encoding="base64" --artifactType="screenshot"' },
        { description: 'Save binary file', command: 'file_save --content=<buffer> --filename="file.bin" --encoding="binary"' },
        { description: 'Save to specific session', command: 'file_save --content=<data> --filename="image.png" --sessionId="session123" --artifactType="screenshot"' }
      ],
      usage: 'Save binary data and base64 encoded content to files with proper session organization'
    };
  }

  static async execute(params: FileSaveParams, _context?: ContinuumContext): Promise<CommandResult> {
    try {
      // 1. Process content based on encoding
      let processedContent: Buffer;
      
      if (Buffer.isBuffer(params.content)) {
        processedContent = params.content;
      } else if (typeof params.content === 'string') {
        if (params.encoding === 'base64') {
          // Handle base64 encoded content
          processedContent = Buffer.from(params.content, 'base64');
        } else if (params.encoding === 'binary') {
          // Handle binary string content
          processedContent = Buffer.from(params.content, 'binary');
        } else {
          // Default to utf8 for text content
          processedContent = Buffer.from(params.content, 'utf8');
        }
      } else {
        throw new Error('Content must be a string or Buffer');
      }

      // 2. Default artifact type to screenshot if not specified
      const artifactType = params.artifactType || ArtifactType.SCREENSHOT;

      // 3. Get target directory (defaults to screenshots directory for session)
      const targetPath = await this.getTargetPath({
        filename: params.filename,
        sessionId: params.sessionId,
        artifactType: artifactType,
        directory: params.directory
      });
      
      // 4. Ensure target directory exists
      await this.ensureDirectoryExists(path.dirname(targetPath));
      
      // 5. Write file using ContinuumFileSystemDaemon delegation
      await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.WRITE_FILE, {
        path: targetPath,
        content: processedContent,
        encoding: undefined // Binary data, no encoding
      });
      
      // 6. Log the save operation via daemon delegation
      await this.logFileOperation('save', targetPath, {
        artifactType: artifactType,
        sessionId: params.sessionId,
        marshalId: params.marshalId,
        size: processedContent.length,
        originalEncoding: params.encoding,
        contentType: this.detectContentType(params.filename)
      });
      
      return this.createSuccessResult(
        `File saved successfully: ${params.filename}`,
        {
          filename: params.filename,
          filepath: targetPath,
          size: processedContent.length,
          artifactType: artifactType,
          sessionId: params.sessionId,
          marshalId: params.marshalId,
          contentType: this.detectContentType(params.filename),
          encoding: params.encoding || 'binary',
          timestamp: new Date().toISOString()
        }
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`File save failed: ${errorMessage}`);
    }
  }

  /**
   * Detect content type based on file extension
   */
  private static detectContentType(filename: string): string {
    const ext = path.extname(filename).toLowerCase();
    
    switch (ext) {
      case '.png': return 'image/png';
      case '.jpg':
      case '.jpeg': return 'image/jpeg';
      case '.gif': return 'image/gif';
      case '.svg': return 'image/svg+xml';
      case '.webp': return 'image/webp';
      case '.pdf': return 'application/pdf';
      case '.mp4': return 'video/mp4';
      case '.webm': return 'video/webm';
      case '.json': return 'application/json';
      case '.txt': return 'text/plain';
      case '.html': return 'text/html';
      case '.css': return 'text/css';
      case '.js': return 'application/javascript';
      case '.ts': return 'application/typescript';
      default: return 'application/octet-stream';
    }
  }

  // All file path and directory methods inherited from BaseFileCommand
}