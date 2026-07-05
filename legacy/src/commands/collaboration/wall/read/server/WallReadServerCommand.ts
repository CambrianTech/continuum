/**
 * wall/read Server Command
 *
 * Reads documents from room walls with optional TOC generation
 */

import { WallReadCommand } from '../shared/WallReadCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WallReadParams, WallReadResult } from '../../shared/WallTypes';
import { WallManager } from '@system/storage/core/WallManager';

export class WallReadServerCommand extends WallReadCommand {
  private wallManager: WallManager;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/wall/read', context, subpath, commander);
    this.wallManager = new WallManager();
  }

  async execute(params: WallReadParams): Promise<WallReadResult> {
    try {
      // Parse line range if provided as string (e.g., "10-50")
      let startLine = params.startLine;
      let endLine = params.endLine;

      if (params.lines) {
        const parts = params.lines.split('-');
        startLine = parseInt(parts[0], 10);
        endLine = parts[1] ? parseInt(parts[1], 10) : undefined;
      }

      // Read document using WallManager
      const readResult = await this.wallManager.readDocument(
        params.room,
        params.doc,
        startLine,
        endLine,
        params.includeMetadata || false
      );

      // Get room info for result
      const roomInfo = await this.wallManager.resolveRoomPath(params.room);

      let outputContent: string;

      // If TOC requested, generate and format it
      if (params.toc) {
        const toc = await this.wallManager.generateTOC(readResult.content);
        // Format matching spec: "## 1. Permission Levels (line 5)"
        const tocLines = toc.map(entry => {
          const prefix = '#'.repeat(entry.level);
          return `${prefix} ${entry.text} (line ${entry.line})`;
        });
        outputContent = tocLines.join('\n');
      } else {
        // Raw content
        outputContent = readResult.content;
      }

      // Return structured result
      return {
        success: true,
        timestamp: new Date().toISOString(),
        content: outputContent,
        roomId: roomInfo.roomId,
        roomName: roomInfo.roomName,
        filePath: readResult.filePath,
        metadata: readResult.lastCommit ? {
          lastCommit: readResult.lastCommit,
          lastAuthor: readResult.lastAuthor!,
          lastModified: readResult.lastModified!,
          lineCount: readResult.lineCount,
          byteCount: readResult.byteCount
        } : undefined,
        context: params.context,
        sessionId: params.sessionId
      };
    } catch (error) {
      const errorMessage = `Failed to read ${params.doc}: ${error instanceof Error ? error.message : String(error)}`;
      throw new Error(errorMessage);
    }
  }
}
