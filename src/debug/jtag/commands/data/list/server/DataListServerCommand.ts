/**
 * Data List Command - Server Implementation
 * 
 * Uses global database storage following ArtifactsDaemon database storage pattern
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataListParams, DataListResult } from '../shared/DataListTypes';
import { createDataListResultFromParams } from '../shared/DataListTypes';
import { WorkingDirConfig } from '../../../../system/core/config/WorkingDirConfig';

export class DataListServerCommand extends CommandBase<DataListParams, DataListResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult> {
    console.debug(`🗄️ DATA SERVER: Listing ${params.collection} from global database`);
    
    try {
      // Use global database path following ArtifactsDaemon database storage pattern
      // Database storage type uses: .continuum/database/{relativePath}
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const collectionDir = path.join(continuumPath, 'database', params.collection);
      
      try {
        const files = await fs.readdir(collectionDir);
        const items = [];
        
        for (const file of files) {
          if (file.endsWith('.json')) {
            try {
              const data = await fs.readFile(path.join(collectionDir, file), 'utf-8');
              items.push(JSON.parse(data));
            } catch (error) {
              console.warn(`Failed to read ${file}:`, error);
            }
          }
        }
        
        const limitedItems = params.limit ? items.slice(0, params.limit) : items;
        
        console.debug(`✅ DATA SERVER: Listed ${limitedItems.length} items from ${params.collection} (global database: ${collectionDir})`);
        
        return createDataListResultFromParams(params, {
          success: true,
          items: limitedItems,
          count: limitedItems.length
        });
        
      } catch (error: any) {
        console.debug(`ℹ️ DATA SERVER: Collection ${params.collection} not found in global database`);
        return createDataListResultFromParams(params, {
          success: true,
          items: [],
          count: 0
        });
      }
      
    } catch (error: any) {
      console.error(`❌ DATA SERVER: List failed:`, error.message);
      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: error.message
      });
    }
  }
}