/**
 * Data Schema Browser Command - Entity Schema Introspection
 *
 * Browser-side proxy that routes schema requests to server
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { DataSchemaParams, DataSchemaResult } from '../shared/DataSchemaTypes';

export class DataSchemaBrowserCommand extends CommandBase<DataSchemaParams, DataSchemaResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('data-schema', context, subpath, commander);
  }

  async execute(params: DataSchemaParams): Promise<DataSchemaResult> {
    console.log(`🔍 DataSchema (Browser): Requesting schema for collection "${params.collection}"`);

    // Delegate to server-side command for actual schema extraction
    return await this.remoteExecute(params);
  }
}