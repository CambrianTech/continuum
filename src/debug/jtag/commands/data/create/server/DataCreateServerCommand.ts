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
    console.log(`🔍 DATA SERVER: Received params:`, JSON.stringify(params, null, 2));
    
    // Cast to DataCreateParams to handle CLI parameter structure
    const dataParams = params as DataCreateParams;
    console.log(`🗄️ DATA SERVER: Creating ${dataParams.collection} record`);
    
    try {
      const id = dataParams.id || generateUUID();
      
      // Follow session-based path structure like FileSaveServerCommand
      const sessionId = dataParams.sessionId;
      const basePath = `.continuum/jtag/sessions/user/${sessionId}`;
      const dataDir = path.resolve(basePath, 'data');
      const collectionDir = path.join(dataDir, dataParams.collection);
      
      // Ensure directories exist
      await fs.mkdir(collectionDir, { recursive: true });
      
      // Parse data if it's a string (from CLI)
      let parsedData = dataParams.data;
      if (typeof dataParams.data === 'string') {
        try {
          parsedData = JSON.parse(dataParams.data);
        } catch (e) {
          console.warn(`🔧 DATA SERVER: Using string data directly (not JSON)`);
        }
      }
      
      const record = {
        id,
        collection: dataParams.collection,
        data: parsedData,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        version: 1
      };
      
      const filePath = path.join(collectionDir, `${id}.json`);
      await fs.writeFile(filePath, JSON.stringify(record, null, 2));
      
      console.log(`✅ DATA SERVER: Created ${dataParams.collection}/${id}`);
      
      return createDataCreateResultFromParams(dataParams, {
        success: true,
        id
      });
      
    } catch (error: any) {
      console.error(`❌ DATA SERVER: Create failed:`, error.message);
      return createDataCreateResultFromParams(dataParams, {
        success: false,
        error: error.message
      });
    }
  }
}