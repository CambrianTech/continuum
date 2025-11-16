/**
 * Codebase Index Command Types
 *
 * Index TypeScript and Markdown files with domain-specific embeddings
 * for semantic code search via RAG
 */

import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { CodeExportType } from '../../../../../system/data/entities/CodeIndexEntity';

/**
 * Parameters for rag/index-codebase command
 */
export interface CodebaseIndexParams extends CommandParams {
  /** Directories or files to index (relative to repo root) */
  paths: string[];

  /** File types to index (default: ['typescript', 'markdown']) */
  fileTypes?: Array<'typescript' | 'markdown' | 'javascript' | 'json'>;

  /** Recurse into subdirectories (default: true) */
  recursive?: boolean;

  /** Update existing entries (default: true) */
  updateExisting?: boolean;

  /** Generate embeddings (default: true) */
  generateEmbeddings?: boolean;

  /** Embedding model to use (default: auto-select by file type) */
  embeddingModel?: string;

  /** Verbose output (default: false) */
  verbose?: boolean;
}

/**
 * Result from rag/index-codebase command
 */
export interface CodebaseIndexResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;

  /** Summary statistics */
  readonly summary: {
    filesIndexed: number;
    filesSkipped: number;
    entriesCreated: number;
    entriesUpdated: number;
    embeddingsGenerated: number;
    durationMs: number;
  };

  /** Per-file breakdown */
  readonly files: Array<{
    filePath: string;
    fileType: 'typescript' | 'markdown' | 'javascript' | 'json';
    entriesCreated: number;
    exportsFound?: Array<{
      exportType: CodeExportType;
      exportName: string;
      startLine: number;
      endLine: number;
    }>;
    embeddingModel?: string;
    status: 'indexed' | 'updated' | 'skipped' | 'error';
    error?: string;
  }>;

  /** Errors encountered */
  readonly errors?: Array<{
    file: string;
    error: string;
  }>;

  /** Warnings */
  readonly warnings?: string[];
}
