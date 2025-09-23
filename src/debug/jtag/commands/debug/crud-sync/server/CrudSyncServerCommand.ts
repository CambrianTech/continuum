/**
 * CRUD Sync Debug Server Command
 *
 * Routes to browser and optionally compares with database data
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { CrudSyncDebugParams, CrudSyncDebugResult } from '../shared/CrudSyncDebugTypes';
import { createCrudSyncDebugResult } from '../shared/CrudSyncDebugTypes';

export class CrudSyncServerCommand extends CommandBase<CrudSyncDebugParams, CrudSyncDebugResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('crud-sync-debug', context, subpath, commander);
  }

  async execute(params: CrudSyncDebugParams): Promise<CrudSyncDebugResult> {
    try {
      console.log(`🔄 CRUD Sync Debug: Routing to browser for widget inspection`);

      // Route to browser for widget data extraction
      const browserResult = await this.remoteExecute<CrudSyncDebugParams, CrudSyncDebugResult>(params);

      if (browserResult.success) {
        console.log(`✅ CRUD Sync: Widget data extracted successfully`);
        console.log(`   - Rooms: ${browserResult.widgets.roomList.itemCount}`);
        console.log(`   - Messages: ${browserResult.widgets.chatWidget.itemCount}`);
        console.log(`   - Users: ${browserResult.widgets.userList.itemCount}`);

        // TODO: Add database comparison here if params.includeDatabase is true

        return browserResult;
      } else {
        console.error(`❌ CRUD Sync: Browser extraction failed: ${browserResult.error}`);
        return browserResult;
      }

    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : String(error);
      console.error(`❌ CRUD Sync: Server command failed: ${errorMessage}`);

      return createCrudSyncDebugResult(this.context, this.context.uuid, {
        success: false,
        error: `CRUD sync debugging failed: ${errorMessage}`,
        debugging: {
          logs: [`Attempted to route to browser for widget inspection`],
          warnings: [],
          errors: [errorMessage]
        }
      });
    }
  }
}