// ISSUES: 0 open, last updated 2025-07-14 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileWriteCommand - Unified file writing with session management
 * 
 * Used by screenshot, artifactory, logging, session management, etc.
 * All file writes go through this command to ensure proper organization
 * 
 * Now uses shared FileOperationParams and FileOperationResult interfaces
 * for consistency across all file operations.
 */

import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand, FileSystemOperation } from '../base/BaseFileCommand';
import { FileOperationParams, FileOperationResult, ArtifactType } from '../../../types/shared/FileOperations';
import type { UUID } from 'crypto';
// Removed direct fs import - now delegates to ContinuumFileSystemDaemon via BaseFileCommand
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
      
      // Get the session base directory from context
      const sessionBasePath = sessionId ? 
        path.join(process.cwd(), '.continuum', 'sessions', 'user', 'shared', sessionId) :
        path.join(process.cwd(), '.continuum');
      
      // Determine target path based on artifact type
      let targetPath: string;
      if (params.artifactType && sessionId) {
        // Use artifact-specific subdirectory in session
        const artifactSubdir = this.getArtifactSubdirectory(params.artifactType);
        targetPath = path.join(sessionBasePath, artifactSubdir, params.filename);
      } else {
        // Use filename as-is (may include relative path)
        targetPath = path.join(sessionBasePath, params.filename);
      }
      
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
        sessionId: sessionId as UUID,
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