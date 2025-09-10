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

// Rust-style config defaults with high values for chat applications
const DEFAULT_CONFIG = {
  database: {
    queryLimit: 100,      // High default for substantial context
    maxBatchSize: 500,    // Safety ceiling
    minLimit: 1,          // Never allow 0 or negative
  }
} as const;

export class DataListServerCommand<T> extends CommandBase<DataListParams, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult<T>> {
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
              const item = JSON.parse(data);
              
              // Apply filter if provided
              if (params.filter) {
                const filterMatches = Object.entries(params.filter).every(([key, value]) => {
                  return item.data && item.data[key] === value;
                });
                if (filterMatches) {
                  items.push(item.data);
                }
              } else {
                items.push(item.data);
              }
            } catch (error) {
              console.warn(`Failed to read ${file}:`, error);
            }
          }
        }
        
        // Safe limit calculation with defensive guards
        const configLimit = (this.context.config as any)?.database?.queryLimit;
        const requestedLimit = params.limit;
        
        // Use Math.max to ensure we never get 0 or negative values
        const safeLimit = Math.max(
          DEFAULT_CONFIG.database.minLimit,
          Math.min(
            requestedLimit || configLimit || DEFAULT_CONFIG.database.queryLimit,
            DEFAULT_CONFIG.database.maxBatchSize
          )
        );
        
        const limitedItems = items.slice(0, safeLimit);
        
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