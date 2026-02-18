/**
 * wall/list Server Command
 *
 * Lists all documents on a room wall
 */

import { WallListCommand } from '../shared/WallListCommand';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { WallListParams, WallListResult } from '../../shared/WallTypes';
import { WallManager } from '@system/storage/core/WallManager';

export class WallListServerCommand extends WallListCommand {
  private wallManager: WallManager;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('collaboration/wall/list', context, subpath, commander);
    this.wallManager = new WallManager();
  }

  async execute(params: WallListParams): Promise<WallListResult> {
    try {
      // List documents using WallManager
      const documents = await this.wallManager.listDocuments(
        params.room,
        params.pattern
      );

      // Resolve room info for result
      const roomInfo = await this.wallManager.resolveRoomPath(params.room);

      return {
        context: params.context,
        sessionId: params.sessionId,
        success: true,
        timestamp: new Date().toISOString(),
        roomId: roomInfo.roomId,
        roomName: roomInfo.roomName,
        documents
      };
    } catch (error) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        timestamp: new Date().toISOString(),
        roomId: '' as any,
        roomName: '',
        documents: [],
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
}
