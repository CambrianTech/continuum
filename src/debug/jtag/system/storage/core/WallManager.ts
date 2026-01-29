/**
 * WallManager - Centralized utility for room wall file operations
 *
 * Handles all file operations for the room wall system:
 * - Reading/writing markdown documents
 * - Git commit automation
 * - TOC generation
 * - Path resolution
 * - Security (path traversal prevention)
 */

import * as path from 'path';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { Commands } from '../../core/shared/Commands';
import type { DataListParams, DataListResult } from '../../../commands/data/list/shared/DataListTypes';
import type { RoomEntity } from '../../data/entities/RoomEntity';
import type { WallDocumentEntity } from '../../data/entities/WallDocumentEntity';
import { COLLECTIONS } from '../../data/config/DatabaseConfig';
import { isRoomUUID, sanitizeDocumentName } from '@commands/collaboration/wall/shared/WallTypes';
import type { FileSaveParams, FileSaveResult } from '../../../commands/file/save/shared/FileSaveTypes';
import type { FileLoadParams, FileLoadResult } from '../../../commands/file/load/shared/FileLoadTypes';

import { DataList } from '../../../commands/data/list/shared/DataListTypes';
import { FileLoad } from '../../../commands/file/load/shared/FileLoadTypes';
import { FileSave } from '../../../commands/file/save/shared/FileSaveTypes';
/**
 * Room path resolution result
 */
export interface RoomPathInfo {
  roomId: UUID;
  roomName: string;
  wallPath: string;  // Absolute path to room's wall directory
}

/**
 * Document write result
 */
export interface WriteResult {
  filePath: string;
  lineCount: number;
  byteCount: number;
  commitHash?: string;
  commitAuthor: string;
  linesAdded: number;
  linesRemoved: number;
}

/**
 * Document read result
 */
export interface ReadResult {
  content: string;
  filePath: string;
  lineCount: number;
  byteCount: number;
  lastCommit?: string;
  lastAuthor?: string;
  lastModified?: Date;
}

/**
 * TOC entry
 */
export interface TOCEntry {
  level: number;
  text: string;
  line: number;
}

/**
 * Document metadata from git
 */
export interface DocumentMetadata {
  lastCommit: string;
  lastAuthor: string;
  lastModified: Date;
  lineCount: number;
  byteCount: number;
}

/**
 * Git commit result
 */
export interface CommitResult {
  hash: string;
  author: string;
  message: string;
}

export class WallManager {
  private readonly continuumRoot: string;

  constructor(continuumRoot?: string) {
    // Default to .continuum in project root (3 levels up from jtag)
    this.continuumRoot = continuumRoot || path.resolve(__dirname, '../../../.continuum');
  }

  /**
   * Resolve room identifier (name or UUID) to full path info
   */
  async resolveRoomPath(roomNameOrId: UUID | string): Promise<RoomPathInfo> {
    let roomEntity: RoomEntity;

    if (isRoomUUID(roomNameOrId)) {
      // Query by UUID
      const result = await DataList.execute<RoomEntity>({
        collection: 'rooms',
        filter: { id: roomNameOrId },
        limit: 1
      });

      if (!result.items || result.items.length === 0) {
        throw new Error(`Room not found: ${roomNameOrId}`);
      }

      roomEntity = result.items[0];
    } else {
      // Query by name
      const result = await DataList.execute<RoomEntity>({
        collection: 'rooms',
        filter: { name: roomNameOrId },
        limit: 1
      });

      if (!result.items || result.items.length === 0) {
        throw new Error(`Room not found: ${roomNameOrId}`);
      }

      roomEntity = result.items[0];
    }

    const wallPath = path.join(this.continuumRoot, 'shared', 'rooms', roomEntity.id);

    return {
      roomId: roomEntity.id as UUID,
      roomName: roomEntity.name,
      wallPath
    };
  }

  /**
   * Write document to room wall
   */
  async writeDocument(
    room: UUID | string,
    doc: string,
    content: string,
    append: boolean = false,
    author: string = 'System'
  ): Promise<WriteResult> {
    const roomInfo = await this.resolveRoomPath(room);
    const sanitizedDoc = sanitizeDocumentName(doc);
    const filePath = path.join(roomInfo.wallPath, sanitizedDoc);

    // Track line changes for result
    let linesAdded = 0;
    let linesRemoved = 0;
    let oldContent = '';

    // Read old content if file exists (for diff stats)
    try {
      const loadResult = await FileLoad.execute({
        filepath: filePath,
        encoding: 'utf-8'
      });
      oldContent = loadResult.content;
    } catch (error) {
      // File doesn't exist - that's fine for new documents
      oldContent = '';
    }

    // Write content
    const finalContent = append && oldContent ? `${oldContent}\n${content}` : content;

    const saveResult = await FileSave.execute({
      filepath: filePath,
      content: finalContent,
      encoding: 'utf-8',
      createDirs: true
    });
    // No need to check success - Commands.execute() throws on error

    // Calculate line changes
    const oldLines = oldContent ? oldContent.split('\n').length : 0;
    const newLines = finalContent.split('\n').length;
    linesAdded = Math.max(0, newLines - oldLines);
    linesRemoved = Math.max(0, oldLines - newLines);

    // Get file stats
    const lineCount = finalContent.split('\n').length;
    const byteCount = saveResult.bytesWritten;

    // Git commit (skip for now - per "worry about optimizations later")
    const commitHash = undefined;

    return {
      filePath,
      lineCount,
      byteCount,
      commitHash,
      commitAuthor: author,
      linesAdded,
      linesRemoved
    };
  }

  /**
   * Read document from room wall
   */
  async readDocument(
    room: UUID | string,
    doc: string,
    startLine?: number,
    endLine?: number,
    includeMetadata: boolean = false
  ): Promise<ReadResult> {
    const roomInfo = await this.resolveRoomPath(room);
    const sanitizedDoc = sanitizeDocumentName(doc);
    const filePath = path.join(roomInfo.wallPath, sanitizedDoc);

    // Read content - throws if file doesn't exist
    const loadResult = await FileLoad.execute({
      filepath: filePath,
      encoding: 'utf-8'
    });

    let content = loadResult.content;
    const allLines = content.split('\n');

    // Apply line range if specified
    if (startLine !== undefined || endLine !== undefined) {
      const start = (startLine || 1) - 1;  // Convert to 0-indexed
      const end = endLine;  // slice end is exclusive, so this works
      content = allLines.slice(start, end).join('\n');
    }

    // Get file stats
    const lineCount = allLines.length;
    const byteCount = loadResult.bytesRead;

    const result: ReadResult = {
      content,
      filePath,
      lineCount,
      byteCount
    };

    // Add git metadata if requested
    if (includeMetadata) {
      try {
        const metadata = await this.getDocumentMetadata(filePath);
        result.lastCommit = metadata.lastCommit;
        result.lastAuthor = metadata.lastAuthor;
        result.lastModified = metadata.lastModified;
      } catch {
        // No git history - skip metadata
      }
    }

    return result;
  }

  /**
   * List all documents in room wall
   */
  async listDocuments(room: UUID | string, pattern?: string): Promise<WallDocumentInfo[]> {
    const roomInfo = await this.resolveRoomPath(room);

    // Query WallDocumentEntity for this room
    const result = await DataList.execute<WallDocumentEntity>({
      collection: COLLECTIONS.WALL_DOCUMENTS,
      filter: { roomId: roomInfo.roomId }
    });

    let documents = result.items.map((data: WallDocumentEntity) => {
      return {
        name: data.name,
        path: data.filePath,
        lineCount: data.lineCount,
        byteCount: data.byteCount,
        lastCommit: data.lastCommitHash || '',
        lastAuthor: data.lastModifiedBy,
        lastModified: new Date(data.lastModifiedAt)
      };
    });

    // Apply pattern filter if specified
    if (pattern) {
      const regex = this.globToRegex(pattern);
      documents = documents.filter((doc: WallDocumentInfo) => regex.test(doc.name));
    }

    // Already sorted by lastModifiedAt desc (from entity pagination config)
    return documents;
  }

  /**
   * Generate table of contents from markdown content
   */
  async generateTOC(content: string): Promise<TOCEntry[]> {
    const lines = content.split('\n');
    const toc: TOCEntry[] = [];

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const match = line.match(/^(#{1,6})\s+(.+)$/);

      if (match) {
        const level = match[1].length;
        const text = match[2].trim();

        toc.push({
          level,
          text,
          line: i + 1  // 1-indexed line numbers
        });
      }
    }

    return toc;
  }

  /**
   * Get document metadata from git
   * TODO: Implement using git commands via Commands.execute() when needed
   */
  async getDocumentMetadata(filePath: string): Promise<DocumentMetadata> {
    throw new Error('Git metadata not yet implemented - skipped for now');
  }

  /**
   * Commit file to git
   * TODO: Implement using git commands via Commands.execute() when needed
   */
  async gitCommit(
    filePath: string,
    message: string,
    author: string
  ): Promise<CommitResult> {
    // Stub - git integration skipped for now
    return {
      hash: '',
      author,
      message
    };
  }

  /**
   * Convert glob pattern to regex
   */
  private globToRegex(pattern: string): RegExp {
    // Simple glob to regex conversion
    // * = any characters except /
    // ** = any characters including /
    let regex = pattern
      .replace(/\./g, '\\.')
      .replace(/\*\*/g, '🌟')  // Temp marker
      .replace(/\*/g, '[^/]*')
      .replace(/🌟/g, '.*');

    return new RegExp(`^${regex}$`);
  }
}

/**
 * Document info for list results
 */
export interface WallDocumentInfo {
  name: string;
  path: string;
  lineCount: number;
  byteCount: number;
  lastCommit: string;
  lastAuthor: string;
  lastModified: Date;
}
