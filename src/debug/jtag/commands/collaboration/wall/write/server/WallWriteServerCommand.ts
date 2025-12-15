/**
 * wall/write Server Command
 *
 * Writes documents to room walls with git tracking and event emission
 */

import { WallWriteCommand } from '../shared/WallWriteCommand';
import { DATA_COMMANDS } from '@commands/data/shared/DataCommandConstants';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WallWriteParams, WallWriteResult } from '../../shared/WallTypes';
import { WallManager } from '@system/storage/core/WallManager';
import { Events } from '@system/core/shared/Events';
import { Commands } from '@system/core/shared/Commands';
import { WallDocumentEntity } from '@system/data/entities/WallDocumentEntity';
import { COLLECTIONS } from '@system/data/config/DatabaseConfig';
import type { DataListParams, DataListResult } from '@commands/data/list/shared/DataListTypes';
import type { DataCreateParams, DataCreateResult } from '@commands/data/create/shared/DataCreateTypes';
import type { DataUpdateParams, DataUpdateResult } from '@commands/data/update/shared/DataUpdateTypes';
import { sanitizeDocumentName } from '../../shared/WallTypes';

export class WallWriteServerCommand extends WallWriteCommand {
  private wallManager: WallManager;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/wall/write', context, subpath, commander);
    this.wallManager = new WallManager();
  }

  async execute(params: WallWriteParams): Promise<WallWriteResult> {
    try {
      // Get author from params or session
      const author = params.author || 'System';  // TODO: get from session userId

      // Write document using WallManager
      const writeResult = await this.wallManager.writeDocument(
        params.room,
        params.doc,
        params.content,
        params.append || false,
        author
      );

      // Resolve room info for result
      const roomInfo = await this.wallManager.resolveRoomPath(params.room);
      const sanitizedName = sanitizeDocumentName(params.doc);

      // Check if WallDocumentEntity already exists
      const existingDocs = await Commands.execute<DataListParams<WallDocumentEntity>, DataListResult<WallDocumentEntity>>(DATA_COMMANDS.LIST, {
        collection: COLLECTIONS.WALL_DOCUMENTS,
        filter: { roomId: roomInfo.roomId, name: sanitizedName },
        limit: 1
      });

      // Create or update WallDocumentEntity
      if (existingDocs.items && existingDocs.items.length > 0) {
        // Update existing document
        await Commands.execute<DataUpdateParams<WallDocumentEntity>, DataUpdateResult<WallDocumentEntity>>('data/update', {
          collection: COLLECTIONS.WALL_DOCUMENTS,
          id: existingDocs.items[0].id,
          data: {
            lastModifiedBy: author,
            lastModifiedAt: new Date(),
            lineCount: writeResult.lineCount,
            byteCount: writeResult.byteCount,
            lastCommitHash: writeResult.commitHash
          }
        });
      } else {
        // Create new document entity
        const newDoc = new WallDocumentEntity();
        newDoc.roomId = roomInfo.roomId;
        newDoc.name = sanitizedName;
        newDoc.filePath = writeResult.filePath;
        newDoc.createdBy = author;
        newDoc.lastModifiedBy = author;
        newDoc.lastModifiedAt = new Date();
        newDoc.lineCount = writeResult.lineCount;
        newDoc.byteCount = writeResult.byteCount;
        newDoc.lastCommitHash = writeResult.commitHash;

        await Commands.execute<DataCreateParams<WallDocumentEntity>, DataCreateResult<WallDocumentEntity>>('data/create', {
          collection: COLLECTIONS.WALL_DOCUMENTS,
          data: newDoc
        });
      }

      // Generate preview (first 100 chars)
      const preview = params.content.substring(0, 100);

      // Emit event for collaborative awareness
      Events.emit(`wall:document:${roomInfo.roomId}`, {
        room: roomInfo.roomName,
        roomId: roomInfo.roomId,
        doc: params.doc,
        author,
        action: params.append ? 'appended' : 'updated',
        summary: {
          lineCount: writeResult.lineCount,
          byteCount: writeResult.byteCount,
          linesAdded: writeResult.linesAdded,
          linesRemoved: writeResult.linesRemoved
        },
        commitHash: writeResult.commitHash,
        preview
      });

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        timestamp: new Date().toISOString(),
        filePath: writeResult.filePath,
        roomId: roomInfo.roomId,
        roomName: roomInfo.roomName,
        commitHash: writeResult.commitHash,
        commitAuthor: author,
        lineCount: writeResult.lineCount,
        byteCount: writeResult.byteCount,
        message: `${params.append ? 'Appended to' : 'Updated'} ${params.doc} in #${roomInfo.roomName}`
      };
    } catch (error) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        timestamp: new Date().toISOString(),
        filePath: '',
        roomId: '' as any,  // Error case
        roomName: '',
        commitAuthor: params.author || 'System',
        lineCount: 0,
        byteCount: 0,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
}
