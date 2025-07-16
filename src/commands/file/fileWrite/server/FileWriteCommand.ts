// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileWriteCommand - Unified file writing with session management
 * 
 * Used by screenshot, artifactory, logging, session management, etc.
 * All file writes go through this command to ensure proper organization
 */

import { CommandDefinition, ContinuumContext, CommandResult } from '../../../core/base-command/BaseCommand';
import { BaseFileCommand, FileSystemOperation } from '../../shared/BaseFileCommand';
import { ArtifactType, FileOperationParams, FileOperationResult } from '../../shared/FileTypes';
import * as path from 'path';

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

  static async execute(params: FileOperationParams, context?: ContinuumContext): Promise<CommandResult> {
    try {
      // Use session ID from context if not provided in params
      const sessionId = params.sessionId || context?.sessionId;
      
      // Get target path using session management
      const targetPath = await this.getTargetPath({
        filename: params.filename,
        sessionId: sessionId,
        artifactType: params.artifactType,
        directory: (params as any).directory
      });
      
      // Ensure target directory exists
      await this.ensureDirectoryExists(path.dirname(targetPath));
      
      // Write file using ContinuumFileSystemDaemon delegation
      const encoding = params.encoding || (Buffer.isBuffer(params.content) ? undefined : 'utf8');
      await this.delegateToContinuumFileSystemDaemon(FileSystemOperation.WRITE_FILE, {
        path: targetPath,
        content: params.content,
        encoding: encoding
      });
      
      // Log the write operation
      await this.logFileOperation('write', targetPath, {
        artifactType: params.artifactType,
        sessionId: sessionId,
        marshalId: params.marshalId,
        size: Buffer.isBuffer(params.content) ? params.content.length : Buffer.byteLength(params.content, encoding || 'utf8')
      });
      
      // Create standardized file operation result
      const fileResult: FileOperationResult = {
        filename: params.filename,
        filepath: targetPath,
        size: Buffer.isBuffer(params.content) ? params.content.length : Buffer.byteLength(params.content, encoding || 'utf8'),
        artifactType: params.artifactType || ArtifactType.FILE,
        sessionId: sessionId as any,
        timestamp: new Date().toISOString(),
        success: true
      };
      
      return this.createSuccessResult(
        `File written successfully: ${params.filename}`,
        fileResult
      );

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      return this.createErrorResult(`File write failed: ${errorMessage}`);
    }
  }

  // All file path and directory methods inherited from BaseFileCommand
}