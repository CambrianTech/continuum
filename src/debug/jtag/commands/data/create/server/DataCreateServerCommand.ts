/**
 * Data Create Command - Server Implementation
 * 
 * Uses global database storage following ArtifactsDaemon database storage pattern
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';
import { WorkingDirConfig } from '../../../../system/core/config/WorkingDirConfig';

export class DataCreateServerCommand extends CommandBase<DataCreateParams, DataCreateResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }

  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    console.debug(`🗄️ DATA SERVER: Creating ${params.collection} record in global database`);
    
    try {
      const id = params.id ?? generateUUID();

      // Create data record following DataDaemon format
      const dataRecord = {
        id,
        collection: params.collection,
        data: params.data,
        metadata: {
          createdAt: new Date().toISOString(),
          updatedAt: new Date().toISOString(),
          version: 1
        }
      };

      // Use global database path following ArtifactsDaemon database storage type pattern
      // Database storage type uses: .continuum/database/{relativePath}
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const databasePath = `${continuumPath}/database`;
      const filepath = `${databasePath}/${params.collection}/${id}.json`;
      const content = JSON.stringify(dataRecord, null, 2);

      const result = await this.remoteExecute({
        filepath: filepath,
        content: content,
        createDirectories: true,
        context: this.context,
        sessionId: params.sessionId
      }, 'file/save');

      const typedResult = result as any;
      if (!typedResult.success) {
        console.error(`❌ DATA SERVER: Global database write failed:`, typedResult.error);
        return createDataCreateResultFromParams(params, {
          success: false,
          error: typedResult.error || 'Database write failed'
        });
      }
      
      console.debug(`✅ DATA SERVER: Created ${params.collection}/${id} in global database`);

      return createDataCreateResultFromParams(params, {
        success: true,
        id
      });
      
    } catch (error: any) {
      console.error(`❌ DATA SERVER: Create failed:`, error.message);
      return createDataCreateResultFromParams(params, {
        success: false,
        error: error.message
      });
    }
  }
}