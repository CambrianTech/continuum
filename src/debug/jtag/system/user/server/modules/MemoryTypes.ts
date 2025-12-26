/**
 * Memory Types for Hippocampus
 *
 * Defines memory entities, types, and interfaces for PersonaUser memory system.
 * See HIPPOCAMPUS-MEMORY-DESIGN.md for full architecture.
 */

import type { BaseEntityData, ISOString } from '../../../data/domains/CoreTypes';

/**
 * Memory types - categorize different kinds of memories
 */
export enum MemoryType {
  CHAT = 'chat',                    // Chat messages (user interactions)
  OBSERVATION = 'observation',      // System events witnessed
  TASK = 'task',                    // Tasks completed/attempted
  DECISION = 'decision',            // Decisions made (with reasoning)
  TOOL_USE = 'tool-use',            // Tool invocations and results
  ERROR = 'error',                  // Errors encountered (for learning)
  INSIGHT = 'insight'               // Self-generated insights/patterns
}

/**
 * Memory Entity - Core memory storage structure
 *
 * Stores episodic memories for PersonaUsers with importance scoring,
 * temporal tracking, and relationship mapping.
 *
 * Properly extends BaseEntityData for data-daemon compatibility
 */
export interface MemoryEntity extends BaseEntityData {
  // Memory-specific identification
  personaId: string;                // Owner of this memory
  sessionId: string;                // Session where memory originated

  // Memory content
  type: MemoryType;
  content: string;                  // Primary memory content (text)
  context: Record<string, any>;     // Structured context (JSON)

  // Temporal information
  timestamp: ISOString;             // When memory was created
  consolidatedAt?: ISOString;       // When moved to LTM (null if STM only)
  lastAccessedAt?: ISOString;       // Last retrieval time (for LRU)

  // Importance/relevance
  importance: number;               // 0.0-1.0 score (how important to remember)
  accessCount: number;              // Times retrieved (frequently accessed = important)

  // Relationships
  relatedTo: string[];              // Links to other memories (graph structure)
  tags: string[];                   // Searchable tags

  // Metadata
  source: string;                   // Where memory came from (e.g., "chat/send", "ai/generate")

  // Semantic embedding (IEmbeddable pattern)
  embedding?: number[];             // Vector embedding for semantic search
  embeddedAt?: ISOString;           // When embedding was generated
  embeddingModel?: string;          // Model used (e.g., "all-minilm")
}

/**
 * Get embeddable content from a MemoryEntity
 * This follows the IEmbeddable pattern for semantic embedding
 */
export function getMemoryEmbeddableContent(memory: MemoryEntity): string {
  return memory.content;
}

/**
 * Parameters for creating a new memory
 * Omits fields that are auto-generated (id, timestamps, accessCount)
 */
export interface CreateMemoryParams {
  personaId: string;
  sessionId: string;
  type: MemoryType;
  content: string;
  context?: Record<string, any>;
  importance: number;
  relatedTo?: string[];
  tags?: string[];
  source: string;
}

/**
 * Parameters for recalling memories
 */
export interface RecallParams {
  // Filter-based recall (existing)
  types?: MemoryType[];             // Filter by memory types
  tags?: string[];                  // Filter by tags (AND logic)
  minImportance?: number;           // Minimum importance threshold
  limit?: number;                   // Max results to return
  since?: ISOString;                // Only memories after this date
  contextFilter?: Record<string, any>;  // Filter by context fields

  // Semantic search options (new)
  semanticQuery?: string;           // Natural language query for semantic search
  semanticThreshold?: number;       // Minimum similarity 0-1 (default: 0.6)
  hybridMode?: 'semantic' | 'filter' | 'hybrid';  // Search strategy

  // Include private thoughts in results
  includePrivate?: boolean;
}

/**
 * Statistics for memory system
 */
export interface MemoryStats {
  stmSize: number;                  // Current STM buffer size
  stmMaxSize: number;               // STM capacity
  ltmCount: number;                 // Total memories in LTM
  consolidationCount: number;       // Memories consolidated this session
  lastConsolidation?: Date;         // Last consolidation timestamp
}
