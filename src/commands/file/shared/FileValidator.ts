// ISSUES: 0 open, last updated 2025-07-16 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking
/**
 * FileValidator - Shared validation logic for all file operations
 * 
 * Following middle-out architecture pattern:
 * - Shared validation logic used across client, server contexts
 * - Centralized validation rules to prevent inconsistencies
 * - Type-safe validation with detailed error reporting
 */

// Browser-compatible path utilities
const pathUtils = {
  extname: (filename: string) => {
    const lastDot = filename.lastIndexOf('.');
    return lastDot === -1 ? '' : filename.substring(lastDot);
  },
  basename: (filename: string, ext?: string) => {
    const base = filename.split('/').pop() || filename;
    return ext && base.endsWith(ext) ? base.slice(0, -ext.length) : base;
  }
};
import { FileOperationParams, FileValidationResult, ArtifactType } from './FileTypes';

/**
 * File validation constraints
 */
export const FILE_CONSTRAINTS = {
  MAX_FILENAME_LENGTH: 255,
  MAX_FILE_SIZE: 50 * 1024 * 1024, // 50MB
  ALLOWED_EXTENSIONS: ['.png', '.jpg', '.jpeg', '.gif', '.svg', '.txt', '.log', '.json', '.csv', '.md', '.html', '.css', '.js', '.ts'],
  FORBIDDEN_CHARS: ['<', '>', ':', '"', '|', '?', '*', '\0'],
  RESERVED_NAMES: ['CON', 'PRN', 'AUX', 'NUL', 'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9', 'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9']
};

/**
 * Shared file validation logic
 */
export class FileValidator {
  
  /**
   * Validate file operation parameters
   */
  static validateParams(params: FileOperationParams): FileValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    // Validate filename
    const filenameResult = this.validateFilename(params.filename);
    errors.push(...filenameResult.errors);
    warnings.push(...filenameResult.warnings);

    // Validate content
    const contentResult = this.validateContent(params.content);
    errors.push(...contentResult.errors);
    warnings.push(...contentResult.warnings);

    // Validate artifact type
    if (params.artifactType) {
      const artifactResult = this.validateArtifactType(params.artifactType);
      errors.push(...artifactResult.errors);
      warnings.push(...artifactResult.warnings);
    }

    // Validate encoding
    if (params.encoding) {
      const encodingResult = this.validateEncoding(params.encoding);
      errors.push(...encodingResult.errors);
      warnings.push(...encodingResult.warnings);
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate filename
   */
  static validateFilename(filename: string): FileValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!filename || typeof filename !== 'string') {
      errors.push('Filename is required and must be a string');
      return { valid: false, errors, warnings };
    }

    // Check length
    if (filename.length > FILE_CONSTRAINTS.MAX_FILENAME_LENGTH) {
      errors.push(`Filename too long (max ${FILE_CONSTRAINTS.MAX_FILENAME_LENGTH} characters)`);
    }

    // Check for forbidden characters
    const forbiddenChars = FILE_CONSTRAINTS.FORBIDDEN_CHARS.filter(char => filename.includes(char));
    if (forbiddenChars.length > 0) {
      errors.push(`Filename contains forbidden characters: ${forbiddenChars.join(', ')}`);
    }

    // Check for reserved names
    const baseName = pathUtils.basename(filename, pathUtils.extname(filename)).toUpperCase();
    if (FILE_CONSTRAINTS.RESERVED_NAMES.includes(baseName)) {
      errors.push(`Filename uses reserved name: ${baseName}`);
    }

    // Check extension
    const ext = pathUtils.extname(filename).toLowerCase();
    if (ext && !FILE_CONSTRAINTS.ALLOWED_EXTENSIONS.includes(ext)) {
      warnings.push(`File extension ${ext} is not in allowed list`);
    }

    // Check for leading/trailing spaces
    if (filename.trim() !== filename) {
      warnings.push('Filename has leading or trailing spaces');
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate file content
   */
  static validateContent(content: string | Buffer | Uint8Array): FileValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (content === null || content === undefined) {
      errors.push('Content is required');
      return { valid: false, errors, warnings };
    }

    // Check content size
    let size: number;
    if (typeof content === 'string') {
      size = Buffer.byteLength(content, 'utf8');
    } else if (content instanceof Buffer) {
      size = content.length;
    } else if (content instanceof Uint8Array) {
      size = content.length;
    } else {
      errors.push('Content must be string, Buffer, or Uint8Array');
      return { valid: false, errors, warnings };
    }

    if (size > FILE_CONSTRAINTS.MAX_FILE_SIZE) {
      errors.push(`Content too large (max ${FILE_CONSTRAINTS.MAX_FILE_SIZE} bytes)`);
    }

    if (size === 0) {
      warnings.push('Content is empty');
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate artifact type
   */
  static validateArtifactType(artifactType: ArtifactType): FileValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!Object.values(ArtifactType).includes(artifactType)) {
      errors.push(`Invalid artifact type: ${artifactType}`);
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate encoding
   */
  static validateEncoding(encoding: BufferEncoding): FileValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    const validEncodings: BufferEncoding[] = [
      'ascii', 'utf8', 'utf-8', 'utf16le', 'ucs2', 'ucs-2', 'base64', 'base64url', 'latin1', 'binary', 'hex'
    ];

    if (!validEncodings.includes(encoding)) {
      errors.push(`Invalid encoding: ${encoding}`);
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate file path
   */
  static validatePath(filePath: string): FileValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!filePath || typeof filePath !== 'string') {
      errors.push('File path is required and must be a string');
      return { valid: false, errors, warnings };
    }

    // Check for absolute path traversal attempts
    if (filePath.includes('..')) {
      errors.push('Path traversal not allowed');
    }

    // Check for null bytes
    if (filePath.includes('\0')) {
      errors.push('Path contains null bytes');
    }

    // Check path length
    if (filePath.length > 4096) {
      errors.push('Path too long (max 4096 characters)');
    }

    // Basic path normalization for browser compatibility
    const normalizedPath = filePath.replace(/\\/g, '/').replace(/\/+/g, '/');
    if (normalizedPath !== filePath) {
      warnings.push('Path was normalized during validation');
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }

  /**
   * Validate session ID format
   */
  static validateSessionId(sessionId: string): FileValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!sessionId || typeof sessionId !== 'string') {
      errors.push('Session ID is required and must be a string');
      return { valid: false, errors, warnings };
    }

    // Check UUID format (basic validation)
    const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
    if (!uuidPattern.test(sessionId)) {
      errors.push('Session ID must be a valid UUID format');
    }

    return {
      valid: errors.length === 0,
      errors,
      warnings
    };
  }
}