/**
 * File MIME Type Command Types
 * Detects MIME type from file extension or file content
 */

import type { FileParams, FileResult } from '../../shared/FileTypes';
import type { CommandParams } from '../../../../system/core/types/JTAGTypes';

/** File MIME type detection parameters */
export interface FileMimeTypeParams extends CommandParams {
  /** Path to the file */
  readonly filepath: string;
  /** Whether to inspect file content if extension is unknown (default: false) */
  inspectContent?: boolean;
  /** File encoding */
  readonly encoding?: string;
}

export interface FileMimeTypeResult extends FileResult {
  /** Detected MIME type */
  mimeType: string;

  /** Media type category */
  mediaType: 'image' | 'audio' | 'video' | 'document' | 'file';

  /** File extension (without dot) */
  extension: string;

  /** Detection method used */
  detectionMethod: 'extension' | 'content' | 'default';
}

/**
 * Comprehensive MIME type mappings
 * Centralized for reuse across the system
 */
export const MIME_TYPES: Record<string, string> = {
  // Images
  'png': 'image/png',
  'jpg': 'image/jpeg',
  'jpeg': 'image/jpeg',
  'gif': 'image/gif',
  'webp': 'image/webp',
  'avif': 'image/avif',
  'heic': 'image/heic',
  'heif': 'image/heif',
  'svg': 'image/svg+xml',
  'bmp': 'image/bmp',
  'ico': 'image/x-icon',
  'tiff': 'image/tiff',
  'tif': 'image/tiff',

  // Audio
  'mp3': 'audio/mpeg',
  'wav': 'audio/wav',
  'ogg': 'audio/ogg',
  'flac': 'audio/flac',
  'm4a': 'audio/mp4',
  'aac': 'audio/aac',
  'opus': 'audio/opus',

  // Video
  'mp4': 'video/mp4',
  'webm': 'video/webm',
  'mov': 'video/quicktime',
  'avi': 'video/x-msvideo',
  'mkv': 'video/x-matroska',
  'flv': 'video/x-flv',
  'wmv': 'video/x-ms-wmv',
  'm4v': 'video/x-m4v',

  // Documents
  'pdf': 'application/pdf',
  'doc': 'application/msword',
  'docx': 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
  'xls': 'application/vnd.ms-excel',
  'xlsx': 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
  'ppt': 'application/vnd.ms-powerpoint',
  'pptx': 'application/vnd.openxmlformats-officedocument.presentationml.presentation',

  // Text
  'txt': 'text/plain',
  'md': 'text/markdown',
  'csv': 'text/csv',
  'html': 'text/html',
  'htm': 'text/html',
  'css': 'text/css',
  'js': 'text/javascript',
  'json': 'application/json',
  'xml': 'application/xml',
  'yaml': 'text/yaml',
  'yml': 'text/yaml',

  // Archives
  'zip': 'application/zip',
  'tar': 'application/x-tar',
  'gz': 'application/gzip',
  'bz2': 'application/x-bzip2',
  '7z': 'application/x-7z-compressed',
  'rar': 'application/vnd.rar',

  // Code
  'ts': 'text/typescript',
  'tsx': 'text/typescript',
  'py': 'text/x-python',
  'rb': 'text/x-ruby',
  'go': 'text/x-go',
  'rs': 'text/x-rust',
  'c': 'text/x-c',
  'cpp': 'text/x-c++',
  'java': 'text/x-java',
  'php': 'text/x-php',
  'sh': 'application/x-sh',
};

/**
 * Get media type category from MIME type
 */
export function getMediaTypeFromMime(mimeType: string): 'image' | 'audio' | 'video' | 'document' | 'file' {
  if (mimeType.startsWith('image/')) return 'image';
  if (mimeType.startsWith('audio/')) return 'audio';
  if (mimeType.startsWith('video/')) return 'video';
  if (mimeType === 'application/pdf' || mimeType.startsWith('text/')) return 'document';
  return 'file';
}
