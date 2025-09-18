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

// Collection constants
const COLLECTIONS = {
  USERS: 'users',
  ROOMS: 'rooms',
  MESSAGES: 'messages'
} as const;

export class DataListServerCommand<T> extends CommandBase<DataListParams, DataListResult<T>> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-list', context, subpath, commander);
  }

  async execute(params: DataListParams): Promise<DataListResult<T>> {
    console.debug(`🗄️ DATA SERVER: Listing ${params.collection}`);

    try {
      // Use global database path following DataCreateServerCommand pattern
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const databasePath = path.join(continuumPath, 'database');
      const collectionPath = path.join(databasePath, params.collection);

      // Check if collection directory exists
      try {
        await fs.access(collectionPath);
      } catch {
        console.debug(`ℹ️ DATA SERVER: Collection ${params.collection} not found`);
        return createDataListResultFromParams(params, {
          success: true,
          items: [],
          count: 0
        });
      }

      // Read all files from the collection directory
      const files = await fs.readdir(collectionPath);
      const jsonFiles = files.filter(file => file.endsWith('.json'));

      const items: T[] = [];
      const limit = Math.min(params.limit || DEFAULT_CONFIG.database.queryLimit, DEFAULT_CONFIG.database.maxBatchSize);

      // Read and parse JSON files
      for (const file of jsonFiles.slice(0, limit)) {
        try {
          const filePath = path.join(collectionPath, file);
          const content = await fs.readFile(filePath, 'utf-8');
          const data = JSON.parse(content);
          items.push(data);
        } catch (error) {
          console.warn(`⚠️ DATA SERVER: Failed to read ${file}:`, error);
        }
      }

      // Apply simple filtering if provided
      let filteredItems = items;
      if (params.filter && Object.keys(params.filter).length > 0) {
        filteredItems = items.filter(item => {
          for (const [key, value] of Object.entries(params.filter!)) {
            if ((item as any)[key] !== value) {
              return false;
            }
          }
          return true;
        });
      }

      // Apply sorting if provided
      if (params.orderBy && params.orderBy.length > 0) {
        filteredItems.sort((a, b) => {
          for (const sort of params.orderBy!) {
            const aValue = (a as any)[sort.field];
            const bValue = (b as any)[sort.field];

            if (aValue < bValue) return sort.direction === 'asc' ? -1 : 1;
            if (aValue > bValue) return sort.direction === 'asc' ? 1 : -1;
          }
          return 0;
        });
      }

      console.debug(`✅ DATA SERVER: Listed ${filteredItems.length}/${items.length} items from ${params.collection}`);

      return createDataListResultFromParams(params, {
        success: true,
        items: filteredItems,
        count: filteredItems.length
      });

    } catch (error: any) {
      console.error(`❌ DATA SERVER: List operation failed:`, error.message);
      return createDataListResultFromParams(params, {
        success: false,
        items: [],
        count: 0,
        error: error.message
      });
    }
  }
}