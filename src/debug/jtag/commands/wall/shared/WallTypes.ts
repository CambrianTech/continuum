/**
 * Room Wall Commands - Shared Types
 *
 * Collaborative document space for each chat room.
 * Bridges ephemeral chat and formal documentation.
 *
 * Commands: wall/write, wall/read, wall/list, wall/history, wall/diff
 */

import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';

/**
 * Parameters for writing to a room wall
 */
export interface WallWriteParams extends CommandParams {
  /** Room name (e.g., "general") or UUID - auto-detected */
  readonly room: UUID | string;

  /** Document name (relative to room wall) */
  readonly doc: string;

  /** Document content */
  readonly content: string;

  /** Append to existing document (default: false = overwrite) */
  readonly append?: boolean;

  /** Author attribution (auto-filled from session if not provided) */
  readonly author?: string;

  /** Commit message for git */
  readonly commitMessage?: string;
}

/**
 * Result of wall write operation
 */
export interface WallWriteResult extends CommandResult {
  readonly success: boolean;
  readonly timestamp: string;

  /** Full path to document */
  readonly filePath: string;

  /** Room info */
  readonly roomId: UUID;
  readonly roomName: string;

  /** Git commit info */
  readonly commitHash?: string;
  readonly commitAuthor: string;

  /** Document stats */
  readonly lineCount: number;
  readonly byteCount: number;

  readonly error?: string;
  readonly message?: string;
}

/**
 * Parameters for reading from a room wall
 */
export interface WallReadParams extends CommandParams {
  /** Room name or UUID */
  readonly room: UUID | string;

  /** Document name */
  readonly doc: string;

  /** Show table of contents / outline only (default: false) */
  readonly toc?: boolean;

  /** Optional line range (e.g., "10-50") */
  readonly lines?: string;

  /** Alternative: start line number */
  readonly startLine?: number;

  /** Alternative: end line number */
  readonly endLine?: number;

  /** Include git metadata */
  readonly includeMetadata?: boolean;
}

/**
 * Result of wall read operation
 */
export interface WallReadResult extends CommandResult {
  readonly success: boolean;
  readonly timestamp: string;

  /** Document content */
  readonly content: string;

  /** Room info */
  readonly roomId: UUID;
  readonly roomName: string;

  /** Document path */
  readonly filePath: string;

  /** Git metadata (if requested) */
  readonly metadata?: {
    lastCommit: string;
    lastAuthor: string;
    lastModified: Date;
    lineCount: number;
    byteCount: number;
  };

  readonly error?: string;
}

/**
 * Parameters for listing room wall documents
 */
export interface WallListParams extends CommandParams {
  /** Room name or UUID */
  readonly room: UUID | string;

  /** Filter pattern (glob) */
  readonly pattern?: string;
}

/**
 * Document info in wall list
 */
export interface WallDocument {
  /** Document name */
  readonly name: string;

  /** Full path */
  readonly path: string;

  /** File stats */
  readonly lineCount: number;
  readonly byteCount: number;

  /** Git info */
  readonly lastCommit: string;
  readonly lastAuthor: string;
  readonly lastModified: Date;

  /** Protection level (if integrated with lease system) */
  readonly protectionLevel?: 'UNRESTRICTED' | 'PEER_REVIEW' | 'SENIOR_REVIEW' | 'HUMAN_REVIEW';
}

/**
 * Result of wall list operation
 */
export interface WallListResult extends CommandResult {
  readonly success: boolean;
  readonly timestamp: string;

  /** Room info */
  readonly roomId: UUID;
  readonly roomName: string;

  /** Documents on this wall */
  readonly documents: WallDocument[];

  readonly error?: string;
}

/**
 * Parameters for viewing wall document history
 */
export interface WallHistoryParams extends CommandParams {
  /** Room name or UUID */
  readonly room: UUID | string;

  /** Document name */
  readonly doc: string;

  /** Number of commits to show */
  readonly limit?: number;
}

/**
 * Git commit info
 */
export interface WallCommit {
  readonly hash: string;
  readonly author: string;
  readonly date: Date;
  readonly message: string;
  readonly linesAdded: number;
  readonly linesRemoved: number;
}

/**
 * Result of wall history operation
 */
export interface WallHistoryResult extends CommandResult {
  readonly success: boolean;
  readonly timestamp: string;

  /** Room info */
  readonly roomId: UUID;
  readonly roomName: string;

  /** Document path */
  readonly filePath: string;

  /** Git history */
  readonly commits: WallCommit[];

  readonly error?: string;
}

/**
 * Parameters for viewing diff between versions
 */
export interface WallDiffParams extends CommandParams {
  /** Room name or UUID */
  readonly room: UUID | string;

  /** Document name */
  readonly doc: string;

  /** From commit (default: HEAD~1) */
  readonly from?: string;

  /** To commit (default: HEAD) */
  readonly to?: string;
}

/**
 * Result of wall diff operation
 */
export interface WallDiffResult extends CommandResult {
  readonly success: boolean;
  readonly timestamp: string;

  /** Room info */
  readonly roomId: UUID;
  readonly roomName: string;

  /** Document path */
  readonly filePath: string;

  /** Diff content */
  readonly diff: string;

  /** Commit info */
  readonly fromCommit: string;
  readonly toCommit: string;

  readonly error?: string;
}

/**
 * Helper: Detect if room identifier is UUID or name
 */
export function isRoomUUID(room: UUID | string): room is UUID {
  // UUIDv4 pattern: 8-4-4-4-12 hex digits
  const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
  return uuidPattern.test(room);
}

/**
 * Helper: Sanitize document name (prevent path traversal)
 */
export function sanitizeDocumentName(doc: string): string {
  // Remove any path traversal attempts
  const sanitized = doc.replace(/\.\./g, '').replace(/^\/+/, '');

  // Ensure .md extension
  if (!sanitized.endsWith('.md')) {
    return `${sanitized}.md`;
  }

  return sanitized;
}
