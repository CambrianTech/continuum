/**
 * Data Create Command - Server Implementation
 * 
 * Direct filesystem operations following FileSaveServerCommand pattern
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import { generateUUID } from '../../../../system/core/types/CrossPlatformUUID';
import type { DataCreateParams, DataCreateResult } from '../shared/DataCreateTypes';
import { createDataCreateResultFromParams } from '../shared/DataCreateTypes';

export class DataCreateServerCommand extends CommandBase<DataCreateParams, DataCreateResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-create', context, subpath, commander);
  }

  async execute(params: DataCreateParams): Promise<DataCreateResult> {
    // Debug: Show what parameters we actually received
    //console.debug(`🔍 DATA SERVER: Received params:`, JSON.stringify(params, null, 2));
    
    // Cast to DataCreateParams to handle CLI parameter structure

    //console.debug(`🗄️ DATA SERVER: Creating ${params.collection} record`);
    
    try {
      const id = params.id ?? generateUUID();

      // Use file/save command for proper storage - global database for persistent data
      // TODO: This should be configurable per collection (some data should be session-specific)
      const isGlobalCollection = ['chat_messages', 'user_profiles', 'chat_rooms'].includes(params.collection);
      const filepath = isGlobalCollection 
        ? `.continuum/jtag/global-database/${params.collection}/${id}.json`
        : `.continuum/database/${params.collection}/${id}.json`;
      const content = JSON.stringify({
        id,
        collection: params.collection,
        data: params.data,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1
      }, null, 2);

      const result = await this.remoteExecute({
        filepath: filepath,
        content: content,
        createDirectories: true,
        context: this.context,
        sessionId: params.sessionId
      }, 'file/save');

      const typedResult = result as any;
      if (!typedResult.success) {
        console.error(`❌ DATA SERVER: File save failed:`, typedResult.error);
        return createDataCreateResultFromParams(params, {
          success: false,
          error: typedResult.error || 'Unknown file save error'
        });
      }
      
      //console.debug(`✅ DATA SERVER: Created ${params.collection}/${id}`);

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