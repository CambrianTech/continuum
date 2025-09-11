/**
 * Data Read Command - Server Implementation
 */

import * as fs from 'fs/promises';
import * as path from 'path';
import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataReadParams, DataReadResult } from '../shared/DataReadTypes';
import { createDataReadResultFromParams } from '../shared/DataReadTypes';
import { WorkingDirConfig } from '../../../../system/core/config/WorkingDirConfig';

export class DataReadServerCommand extends CommandBase<DataReadParams, DataReadResult> {
  
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-read', context, subpath, commander);
  }

  async execute(params: DataReadParams): Promise<DataReadResult> {
    console.log(`🗄️ DATA SERVER: Reading ${params.collection}/${params.id}`);
    
    try {
      // Use global database path following ArtifactsDaemon database storage pattern
      // Database storage type uses: .continuum/database/{relativePath}
      const continuumPath = WorkingDirConfig.getContinuumPath();
      const databasePath = `${continuumPath}/database`;
      const filePath = path.join(databasePath, params.collection, `${params.id}.json`);
      
      const data = await fs.readFile(filePath, 'utf-8');
      const record = JSON.parse(data);
      
      console.log(`✅ DATA SERVER: Read ${params.collection}/${params.id}`);
      
      return createDataReadResultFromParams(params, {
        success: true,
        data: record,
        found: true
      });
      
    } catch (error: any) {
      console.log(`ℹ️ DATA SERVER: ${params.collection}/${params.id} not found`);
      return createDataReadResultFromParams(params, {
        success: true,
        found: false
      });
    }
  }
}