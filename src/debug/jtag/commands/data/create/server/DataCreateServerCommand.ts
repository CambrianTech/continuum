/**
 * Data Create Command - Server Implementation
 *
 * Uses DataDaemon for proper storage abstraction (SQLite backend)
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';
import type { DataDaemon, DataOperationContext } from '../../../../daemons/data-daemon/shared/DataDaemon';
import { DATABASE_PATHS } from '../../../../system/data/config/DatabaseConfig';

export class DataCreateServerCommand extends CommandBase<DataCreateParams, DataCreateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }

  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    console.debug(`🗄️ DATA SERVER: Creating ${params.collection} record in SQLite`);

    try {
      const id = params.id ?? generateUUID();
      const now = new Date().toISOString();

      // Use SQLite database directly for now - with centralized config
      const sqlite3 = require('sqlite3').verbose();
      const db = new sqlite3.Database(DATABASE_PATHS.SQLITE);

      await new Promise<void>((resolve, reject) => {
        const sql = `INSERT INTO entities (id, collection, data, created_at, updated_at, version, search_content)
                     VALUES (?, ?, ?, ?, ?, ?, ?)`;

        const searchContent = typeof params.data === 'string' ? params.data : JSON.stringify(params.data);

        db.run(sql, [
          id,
          params.collection,
          JSON.stringify(params.data),
          now,
          now,
          1,
          searchContent
        ], function(err: any) {
          db.close();
          if (err) {
            reject(err);
          } else {
            resolve();
          }
        });
      });

      console.debug(`✅ DATA SERVER: Created ${params.collection}/${id} in SQLite`);

      return createDataCreateResultFromParams(params, {
        success: true,
        id
      });

    } catch (error: any) {
      console.error(`❌ DATA SERVER: SQLite create failed:`, error.message);
      return createDataCreateResultFromParams(params, {
        success: false,
        error: error.message
      });
    }
  }
}