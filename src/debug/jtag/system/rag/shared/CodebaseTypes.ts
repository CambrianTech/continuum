/**
 * Codebase RAG Types
 *
 * Types for indexing and querying source code and documentation
 */

import type { UUID } from '../../core/types/CrossPlatformUUID';
import type { BaseEntity } from '../../data/entities/BaseEntity';

/**
 * Type of code export
 */
export type CodeExportType =
  | 'class'
  | 'interface'
  | 'type'
  | 'function'
  | 'const'
  | 'enum'
  | 'namespace'
  | 'module'
  | 'markdown-section'
  | 'markdown-doc';

/**
 * Code index entry stored in database
 * Each entry represents a searchable code snippet
 */
export interface CodeIndexEntry extends BaseEntity {
  // File information
  filePath: string;
  fileType: 'typescript' | 'markdown' | 'javascript' | 'json';

  // Content
  content: string;  // The actual code or documentation text
  summary?: string;  // Optional summary for long content

  // Location within file
  startLine?: number;
  endLine?: number;

  // Export information (for TypeScript)
  exportType?: CodeExportType;
  exportName?: string;  // e.g., "PersonaUser", "Commands"

  // Embeddings for semantic search
  embedding?: number[];  // Vector embedding of content
  embeddingModel?: string;  // e.g., "text-embedding-ada-002"

  // Search metadata
  relevanceScore?: number;  // Computed during query
  lastIndexed: Date;  // When this entry was indexed

  // Additional metadata
  imports?: string[];  // Files/modules this code imports from
  exports?: string[];  // What this code exports
  tags?: string[];  // Additional categorization
}

/**
 * Query options for codebase search
 */
export interface CodebaseQueryOptions {
  query: string;  // Search query
  limit?: number;  // Max results (default: 10)
  fileTypes?: ('typescript' | 'markdown' | 'javascript' | 'json')[];  // Filter by file type
  exportTypes?: CodeExportType[];  // Filter by export type
  scopePath?: string;  // Limit to specific directory (e.g., "/system/user/")
  includeEmbeddings?: boolean;  // Return embeddings in results (default: false)
}

/**
 * Code indexing options
 */
export interface CodeIndexOptions {
  paths: string[];  // Directories or files to index
  fileTypes?: ('typescript' | 'markdown' | 'javascript' | 'json')[];
  recursive?: boolean;  // Recurse into subdirectories (default: true)
  updateExisting?: boolean;  // Update existing entries (default: true)
  generateEmbeddings?: boolean;  // Generate vector embeddings (default: true)
  embeddingModel?: string;  // Embedding model to use
}

/**
 * Indexing result
 */
export interface IndexingResult {
  success: boolean;
  filesIndexed: number;
  entriesCreated: number;
  entriesUpdated: number;
  errors: Array<{ file: string; error: string }>;
  durationMs: number;
}

/**
 * Query result
 */
export interface CodebaseQueryResult {
  success: boolean;
  entries: CodeIndexEntry[];
  totalResults: number;
  durationMs: number;
}
