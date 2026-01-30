/**
 * Index Create Command Types
 *
 * Low-level primitive for storing a single code entry with embeddings
 */

import type { CommandParams, CommandResult, CommandInput} from '../../../../../../system/core/types/JTAGTypes';
import type { CodeExportType } from '../../../../../../system/data/entities/CodeIndexEntity';
import { Commands } from '../../../../../../system/core/shared/Commands';

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

/**
 * IndexCreate — Type-safe command executor
 *
 * Usage:
 *   import { IndexCreate } from '...shared/IndexCreateTypes';
 *   const result = await IndexCreate.execute({ ... });
 */
export const IndexCreate = {
  execute(params: CommandInput<IndexCreateParams>): Promise<IndexCreateResult> {
    return Commands.execute<IndexCreateParams, IndexCreateResult>('ai/rag/index/create', params as Partial<IndexCreateParams>);
  },
  commandName: 'ai/rag/index/create' as const,
} as const;
