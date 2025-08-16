/**
 * Data Update Command - Server Implementation
 * 
 * Direct filesystem operations following working data create pattern
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataUpdateParams, DataUpdateResult } from '../shared/DataUpdateTypes';
import { createDataUpdateResultFromParams } from '../shared/DataUpdateTypes';

export class DataUpdateServerCommand extends CommandBase<DataUpdateParams, DataUpdateResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-update', context, subpath, commander);
  }

  async execute(params: DataUpdateParams): Promise<DataUpdateResult> {
    console.log(`🔄 DATA UPDATE: Updating ${params.collection}/${params.id}`);
    
    try {
      // Follow session-based path structure like DataCreateServerCommand
      const sessionId = params.sessionId;
      const basePath = `.continuum/jtag/sessions/user/${sessionId}`;
      const dataDir = path.resolve(basePath, 'data');
      const collectionDir = path.join(dataDir, params.collection);
      const filePath = path.join(collectionDir, `${params.id}.json`);
      
      // Check if record exists
      let existingRecord;
      try {
        const fileContent = await fs.readFile(filePath, 'utf-8');
        existingRecord = JSON.parse(fileContent);
      } catch (e) {
        return createDataUpdateResultFromParams(params, {
          error: `Record not found: ${params.collection}/${params.id}`,
          found: false
        });
      }
      
      // Parse update data if it's a string (from CLI)
      let updateData = params.data;
      if (typeof params.data === 'string') {
        try {
          updateData = JSON.parse(params.data);
        } catch (e) {
          return createDataUpdateResultFromParams(params, {
            error: `Invalid JSON in data parameter: ${e instanceof Error ? e.message : 'Unknown error'}`,
            found: true
          });
        }
      }
      
      // Merge update data with existing record
      const updatedRecord = {
        ...existingRecord,
        data: {
          ...existingRecord.data,
          ...updateData
        },
        updatedAt: new Date().toISOString(),
        version: params.incrementVersion !== false ? (existingRecord.version || 1) + 1 : existingRecord.version
      };
      
      // Write updated record
      await fs.writeFile(filePath, JSON.stringify(updatedRecord, null, 2));
      
      console.log(`✅ DATA UPDATE: Updated ${params.collection}/${params.id} (v${updatedRecord.version})`);
      
      return createDataUpdateResultFromParams(params, {
        found: true,
        data: updatedRecord,
        previousVersion: existingRecord.version,
        newVersion: updatedRecord.version
      });
      
    } catch (error: any) {
      console.error(`❌ DATA UPDATE: Update failed:`, error.message);
      return createDataUpdateResultFromParams(params, {
        error: error.message,
        found: false
      });
    }
  }
}