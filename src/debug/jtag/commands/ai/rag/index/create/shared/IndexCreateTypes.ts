/**
 * Index Create Command Types
 *
 * Low-level primitive for storing a single code entry with embeddings
 */

import type { CommandParams, CommandResult } from '../../../../../../system/core/types/JTAGTypes';
import type { CodeExportType } from '../../../../../../system/data/entities/CodeIndexEntity';

/**
 * Parameters for creating a code index entry
 */
export interface IndexCreateParams extends CommandParams {
  // File information
  filePath: string;
  fileType: 'typescript' | 'markdown' | 'javascript' | 'json';

  // Content
  content: string;
  summary?: string;

  // Location within file
  startLine?: number;
  endLine?: number;

  // Export information (for TypeScript/JavaScript)
  exportType?: CodeExportType;
  exportName?: string;

  // Vector embeddings for semantic search
  embedding?: number[];
  embeddingModel?: string;

  // Additional metadata
  imports?: string[];
  exports?: string[];
  tags?: string[];
}

/**
 * Result of creating a code index entry
 */
export interface IndexCreateResult extends CommandResult {
  readonly success: boolean;
  readonly error?: string;
  readonly entryId?: string;  // UUID of created entry
  readonly indexed: boolean;  // Whether the entry was successfully indexed
}
