// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileSaveCommand - Save binary data (like screenshots) to files
 * 
 * Handles base64 encoded content and binary data, saves to the screenshots 
 * directory for the current session. Built on the unified file system architecture.
 */

import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand, FileSystemOperation } from '../shared/BaseFileCommand';
import { ArtifactType, FileSaveClientResult } from '../shared/FileTypes';
import * as path from 'path';

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

  static async execute(params: any, context?: ContinuumContext): Promise<CommandResult> {
    try {
      // 1. Get session ID from context if not provided in params
      const sessionId = params.sessionId || context?.sessionId;

      // 2. Process content based on encoding
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

      // 3. Default artifact type to screenshot if not specified
      const artifactType = params.artifactType || ArtifactType.SCREENSHOT;

      // 4. Get target directory (defaults to screenshots directory for session)
      const targetPath = await this.getTargetPath({
        filename: params.filename,
        sessionId: sessionId,
        artifactType: artifactType,
        directory: params.directory
      });
      
      // 5. Ensure target directory exists
      await this.ensureDirectoryExists(path.dirname(targetPath));
      
      // 6. Write file using ContinuumFileSystemDaemon delegation
      await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.WRITE_FILE, {
        path: targetPath,
        content: processedContent,
        encoding: undefined // Binary data, no encoding
      });
      
      // 7. Log the save operation via daemon delegation
      await this.logFileOperation('save', targetPath, {
        artifactType: artifactType,
        sessionId: sessionId,
        marshalId: params.marshalId,
        size: processedContent.length,
        originalEncoding: params.encoding,
        contentType: this.detectContentType(params.filename)
      });
      
      // 8. Create result using the specific FileSaveClientResult type
      const result: FileSaveClientResult = {
        success: true,
        data: {
          filename: params.filename,
          filepath: targetPath,
          size: processedContent.length,
          artifactType: artifactType,
          sessionId: sessionId as any,
          timestamp: new Date().toISOString()
        }
      };
      
      return this.createSuccessResult(
        `File saved successfully: ${params.filename}`,
        result
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
    const extWithDot = path.extname(filename).toLowerCase();
    const ext = extWithDot.substring(1); // Remove the dot
    
    // No extension
    if (!ext) return 'application/octet-stream';
    
    // Simple pattern-based detection
    if (['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(ext)) {
      return `image/${ext === 'jpg' ? 'jpeg' : ext}`;
    }
    if (['mp4', 'webm'].includes(ext)) {
      return `video/${ext}`;
    }
    if (['html', 'css', 'txt'].includes(ext)) {
      return `text/${ext === 'txt' ? 'plain' : ext}`;
    }
    
    // Special cases
    const specialTypes: Record<string, string> = {
      svg: 'image/svg+xml',
      pdf: 'application/pdf',
      json: 'application/json',
      js: 'application/javascript',
      ts: 'application/typescript'
    };
    
    return specialTypes[ext] || 'application/octet-stream';
  }

  // All file path and directory methods inherited from BaseFileCommand
}