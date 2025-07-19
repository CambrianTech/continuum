// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileWriteCommand - Unified file writing with session management
 * 
 * Used by screenshot, artifactory, logging, session management, etc.
 * All file writes go through this command to ensure proper organization
 */

import { CommandDefinition, ContinuumContext, CommandResult } from '../../core/base-command/BaseCommand';
import { BaseFileCommand, FileSystemOperation } from '../shared/BaseFileCommand';
import { ArtifactType, FileOperationResult } from '../shared/FileTypes';
import * as path from 'path';
import type { UUID } from 'crypto';

// ✅ STRONGLY TYPED PARAMETERS - Eliminates 'any' types
// Extends FileOperationParams with additional FileWriteCommand-specific options
interface FileWriteParameters {
  content: string | Buffer | Uint8Array;
  filename: string;
  artifactType?: ArtifactType;
  encoding?: BufferEncoding;
  sessionId?: UUID;
  marshalId?: string;
  directory?: string; // Directory override for file location
}

/**
 * Type guard for FileWriteParameters
 */
function validateFileWriteParameters(params: unknown): params is FileWriteParameters {
  if (typeof params !== 'object' || params === null) {
    return false;
  }
  
  const obj = params as Record<string, unknown>;
  
  // Required fields
  if (!obj.content || (typeof obj.content !== 'string' && !Buffer.isBuffer(obj.content) && !(obj.content instanceof Uint8Array))) {
    return false;
  }
  if (typeof obj.filename !== 'string') {
    return false;
  }
  
  // Optional fields - validate types if present
  if (obj.artifactType !== undefined && !Object.values(ArtifactType).includes(obj.artifactType as ArtifactType)) {
    return false;
  }
  if (obj.encoding !== undefined && typeof obj.encoding !== 'string') {
    return false;
  }
  if (obj.sessionId !== undefined && typeof obj.sessionId !== 'string') {
    return false;
  }
  if (obj.marshalId !== undefined && typeof obj.marshalId !== 'string') {
    return false;
  }
  if (obj.directory !== undefined && typeof obj.directory !== 'string') {
    return false;
  }
  
  return true;
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

  /**
   * ✅ JOEL'S CLEAN APPROACH - Single execute method with internal typing
   * Validation at the top, then typed params for rest of method
   */
  static async execute(parameters: unknown, context: ContinuumContext): Promise<CommandResult> {
    // === VALIDATION SECTION (top of method) ===
    const parsedParams = this.preprocessParameters(parameters); // ✅ Automatic CLI parsing
    
    if (!validateFileWriteParameters(parsedParams)) {
      return { success: false, error: 'Invalid file write parameters. content (string|Buffer|Uint8Array) and filename (string) are required.', timestamp: new Date().toISOString() };
    }
    
    // === TYPED BUSINESS LOGIC SECTION (rest of method) ===
    // Now params is strongly typed for the entire rest of the method
    const params = parsedParams as FileWriteParameters;
    
    try {
      // Use session ID from context if not provided in params
      const sessionId = params.sessionId || context.sessionId;
      
      // Get target path using session management
      const targetPath = await this.getTargetPath({
        filename: params.filename,
        sessionId: sessionId,
        artifactType: params.artifactType,
        directory: params.directory // ✅ No 'any' cast needed - strongly typed
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
        sessionId: sessionId as UUID, // ✅ Proper type assertion - sessionId is UUID
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