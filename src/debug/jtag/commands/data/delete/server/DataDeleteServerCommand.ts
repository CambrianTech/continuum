/**
 * Data Delete Command - Server Implementation
 * 
 * Direct filesystem operations following working data create pattern
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataDeleteParams, DataDeleteResult } from '../shared/DataDeleteTypes';
import { createDataDeleteResultFromParams } from '../shared/DataDeleteTypes';

export class DataDeleteServerCommand extends CommandBase<DataDeleteParams, DataDeleteResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-delete', context, subpath, commander);
  }

  async execute(params: DataDeleteParams): Promise<DataDeleteResult> {
    console.log(`🗑️ DATA DELETE: Deleting ${params.collection}/${params.id}`);
    
    try {
      // Follow session-based path structure like DataCreateServerCommand
      const sessionId = params.sessionId;
      const basePath = `.continuum/jtag/sessions/user/${sessionId}`;
      const dataDir = path.resolve(basePath, 'data');
      const collectionDir = path.join(dataDir, params.collection);
      const filePath = path.join(collectionDir, `${params.id}.json`);
      
      // Check if record exists before deleting
      let existingRecord;
      try {
        const fileContent = await fs.readFile(filePath, 'utf-8');
        existingRecord = JSON.parse(fileContent);
      } catch (e) {
        return createDataDeleteResultFromParams(params, {
          error: `Record not found: ${params.collection}/${params.id}`,
          found: false
        });
      }
      
      // Delete the file
      await fs.unlink(filePath);
      
      console.log(`✅ DATA DELETE: Deleted ${params.collection}/${params.id}`);
      
      return createDataDeleteResultFromParams(params, {
        found: true,
        deleted: true
      });
      
    } catch (error: any) {
      console.error(`❌ DATA DELETE: Delete failed:`, error.message);
      return createDataDeleteResultFromParams(params, {
        error: error.message,
        found: false
      });
    }
  }
}