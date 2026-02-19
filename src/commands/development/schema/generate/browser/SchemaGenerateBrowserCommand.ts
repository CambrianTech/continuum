/**
 * Schema Generate Command - Browser Implementation
 *
 * Server-only command (requires TypeScript compiler and file system).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import type { SchemaGenerateParams, SchemaGenerateResult } from '../shared/SchemaGenerateTypes';
import { createSchemaGenerateResult } from '../shared/SchemaGenerateTypes';

export class SchemaGenerateBrowserCommand extends CommandBase<SchemaGenerateParams, SchemaGenerateResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/schema/generate', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<SchemaGenerateResult> {
    const schemaParams = params as SchemaGenerateParams;

    console.log(`🌐 BROWSER: schema/generate is server-only`);

    // This command requires TypeScript compiler and file system (server-only)
    return createSchemaGenerateResult(schemaParams, {
      success: false,
      error: 'schema/generate is a server-only command'
    });
  }
}
