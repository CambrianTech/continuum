/**
 * Data List Command - Server Implementation
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataListParams, DataListResult } from '../shared/DataListTypes';
import { createDataListResultFromParams } from '../shared/DataListTypes';

export class DataListServerCommand extends CommandBase<DataListParams, DataListResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult> {
    console.log(`🗄️ DATA SERVER: Listing ${params.collection}`);
    
    try {
      // Follow session-based path structure like FileSaveServerCommand
      const sessionId = params.sessionId;
      const basePath = `.continuum/jtag/sessions/user/${sessionId}`;
      const dataDir = path.resolve(basePath, 'data');
      const collectionDir = path.join(dataDir, params.collection);
      
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
        
        console.log(`✅ DATA SERVER: Listed ${limitedItems.length} items from ${params.collection}`);
        
        return createDataListResultFromParams(params, {
          success: true,
          items: limitedItems,
          count: limitedItems.length
        });
        
      } catch (error: any) {
        console.log(`ℹ️ DATA SERVER: Collection ${params.collection} not found`);
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