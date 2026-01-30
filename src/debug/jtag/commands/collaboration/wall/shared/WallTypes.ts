/**
 * Room Wall Commands - Shared Types
 *
 * Collaborative document space for each chat room.
 * Bridges ephemeral chat and formal documentation.
 *
 * Commands: wall/write, wall/read, wall/list, wall/history, wall/diff
 */

import type { UUID } from '@system/core/types/CrossPlatformUUID';
import type { CommandParams, CommandResult, CommandInput} from '@system/core/types/JTAGTypes';
import { isUUID } from '@system/routing/RoutingService';
import { Commands } from '../../../../system/core/shared/Commands';

/**
 * Parameters for writing to a room wall
 */
export interface WallWriteParams extends CommandParams {
  /** Room name (e.g., "general") or UUID - auto-detected from current room if not provided */
  readonly room?: UUID | string;

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
 * Delegates to RoutingService isUUID for single source of truth
 */
export function isRoomUUID(room: UUID | string): room is UUID {
  return isUUID(room);
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

/**
 * WallWrite — Type-safe command executor
 *
 * Usage:
 *   import { WallWrite } from '...shared/WallWriteTypes';
 *   const result = await WallWrite.execute({ ... });
 */
export const WallWrite = {
  execute(params: CommandInput<WallWriteParams>): Promise<WallWriteResult> {
    return Commands.execute<WallWriteParams, WallWriteResult>('collaboration/wall/write', params as Partial<WallWriteParams>);
  },
  commandName: 'collaboration/wall/write' as const,
} as const;

/**
 * WallRead — Type-safe command executor
 *
 * Usage:
 *   import { WallRead } from '...shared/WallReadTypes';
 *   const result = await WallRead.execute({ ... });
 */
export const WallRead = {
  execute(params: CommandInput<WallReadParams>): Promise<WallReadResult> {
    return Commands.execute<WallReadParams, WallReadResult>('collaboration/wall/read', params as Partial<WallReadParams>);
  },
  commandName: 'collaboration/wall/read' as const,
} as const;

/**
 * WallList — Type-safe command executor
 *
 * Usage:
 *   import { WallList } from '...shared/WallListTypes';
 *   const result = await WallList.execute({ ... });
 */
export const WallList = {
  execute(params: CommandInput<WallListParams>): Promise<WallListResult> {
    return Commands.execute<WallListParams, WallListResult>('collaboration/wall/list', params as Partial<WallListParams>);
  },
  commandName: 'collaboration/wall/list' as const,
} as const;

/**
 * WallHistory — Type-safe command executor
 *
 * Usage:
 *   import { WallHistory } from '...shared/WallHistoryTypes';
 *   const result = await WallHistory.execute({ ... });
 */
export const WallHistory = {
  execute(params: CommandInput<WallHistoryParams>): Promise<WallHistoryResult> {
    return Commands.execute<WallHistoryParams, WallHistoryResult>('collaboration/wall/history', params as Partial<WallHistoryParams>);
  },
  commandName: 'collaboration/wall/history' as const,
} as const;

/**
 * WallDiff — Type-safe command executor
 *
 * Usage:
 *   import { WallDiff } from '...shared/WallDiffTypes';
 *   const result = await WallDiff.execute({ ... });
 */
export const WallDiff = {
  execute(params: CommandInput<WallDiffParams>): Promise<WallDiffResult> {
    return Commands.execute<WallDiffParams, WallDiffResult>('collaboration/wall/diff', params as Partial<WallDiffParams>);
  },
  commandName: 'collaboration/wall/diff' as const,
} as const;
