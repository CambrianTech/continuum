/**
 * CodeIndexEntity - Indexed code and documentation entries
 *
 * Each entry represents a searchable piece of code or documentation:
 * - TypeScript exports (classes, interfaces, functions, types)
 * - Markdown sections (documentation)
 * - With embeddings for semantic vector search
 */

import { BaseEntity } from './BaseEntity';
import { COLLECTIONS } from '../../shared/Constants';
import {
  TextField,
  EnumField,
  JsonField,
  DateField,
  NumberField,
  TEXT_LENGTH
} from '../decorators/FieldDecorators';

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
 * Code index entry
 * Stored in code_index collection
 */
export class CodeIndexEntity extends BaseEntity {
  // Single source of truth for collection name
  static readonly collection = 'code_index';

  // File information
  @TextField({ index: true })
  filePath!: string;  // Relative path from repo root

  @EnumField({ index: true })
  fileType!: 'typescript' | 'markdown' | 'javascript' | 'json';

  // Content
  @TextField({ maxLength: TEXT_LENGTH.MEDIUM })
  content!: string;  // The actual code or documentation text

  @TextField({ maxLength: TEXT_LENGTH.SHORT, nullable: true })
  summary?: string;  // Optional summary for long content

  // Location within file
  @NumberField({ nullable: true })
  startLine?: number;

  @NumberField({ nullable: true })
  endLine?: number;

  // Export information (for TypeScript/JavaScript)
  @EnumField({ nullable: true, index: true })
  exportType?: CodeExportType;

  @TextField({ nullable: true, index: true })
  exportName?: string;  // e.g., "PersonaUser", "Commands"

  // Vector embeddings for semantic search
  @JsonField({ nullable: true })
  embedding?: number[];  // Vector embedding of content

  @TextField({ nullable: true })
  embeddingModel?: string;  // e.g., "qwen3-embedding", "nomic-embed-text"

  // Search metadata
  @NumberField({ nullable: true })
  relevanceScore?: number;  // Computed during query (not stored)

  @DateField({ index: true })
  lastIndexed!: Date;  // When this entry was last indexed

  // Additional metadata
  @JsonField({ nullable: true })
  imports?: string[];  // Files/modules this code imports from

  @JsonField({ nullable: true })
  exports?: string[];  // What this code exports

  @JsonField({ nullable: true })
  tags?: string[];  // Additional categorization

  /**
   * Implement BaseEntity abstract method
   */
  get collection(): string {
    return CodeIndexEntity.collection;
  }

  /**
   * Implement BaseEntity abstract method - validate code index entry
   */
  validate(): { success: boolean; error?: string } {
    // Required fields validation
    if (!this.filePath?.trim()) {
      return { success: false, error: 'CodeIndex filePath is required' };
    }

    if (!this.fileType) {
      return { success: false, error: 'CodeIndex fileType is required' };
    }

    if (!this.content?.trim()) {
      return { success: false, error: 'CodeIndex content is required' };
    }

    if (!this.lastIndexed) {
      return { success: false, error: 'CodeIndex lastIndexed is required' };
    }

    // Validation passed
    return { success: true };
  }
}

/**
 * Collection name for code index entries
 */
export const CODE_INDEX_COLLECTION = COLLECTIONS.CODE_INDEX;
