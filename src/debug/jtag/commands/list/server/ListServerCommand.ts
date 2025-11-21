/**
 * List Command - Server Implementation
 * 
 * Discovers and returns available commands from the CommandDaemon system.
 * Essential command for client command discovery.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { type ListParams, type ListResult, type CommandSignature, createListResultFromParams } from '../shared/ListTypes';
import { readFileSync, existsSync } from 'fs';
import { join } from 'path';

// Generated schemas interface
interface GeneratedSchemas {
  generated: string;
  version: string;
  commands: Array<{
    name: string;
    description: string;
    params: Record<string, { type: string; required: boolean; description?: string }>;
  }>;
}

export class ListServerCommand extends CommandBase<ListParams, ListResult> {
  private static generatedSchemas: GeneratedSchemas | null = null;
  private static schemasLoadError: string | null = null;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('list', context, subpath, commander);

    // Load schemas once on first instantiation
    if (ListServerCommand.generatedSchemas === null && ListServerCommand.schemasLoadError === null) {
      ListServerCommand.loadGeneratedSchemas();
    }
  }

  /**
   * Load generated command schemas from JSON file
   */
  private static loadGeneratedSchemas(): void {
    try {
      const schemaPath = join(process.cwd(), 'generated-command-schemas.json');

      if (!existsSync(schemaPath)) {
        console.warn('⚠️ LIST: generated-command-schemas.json not found. Run: npx tsx generator/generate-command-schemas.ts');
        ListServerCommand.schemasLoadError = 'Schemas file not found';
        return;
      }

      const content = readFileSync(schemaPath, 'utf-8');
      ListServerCommand.generatedSchemas = JSON.parse(content);
      console.log(`✅ LIST: Loaded ${ListServerCommand.generatedSchemas?.commands.length ?? 0} command schemas from generated file`);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      console.error('❌ LIST: Failed to load generated schemas:', errorMessage);
      ListServerCommand.schemasLoadError = errorMessage;
    }
  }

  /**
   * Server discovers available commands from the CommandDaemon
   */
  async execute(params: JTAGPayload): Promise<ListResult> {
    const listParams = params as ListParams;

    console.log(`📋 SERVER: Listing available commands`);

    try {
      // Get commands from CommandDaemon
      const availableCommands = this.commander.commands;
      const commandSignatures: CommandSignature[] = [];

      // Convert CommandDaemon commands to CommandSignature format
      for (const [commandName, command] of availableCommands.entries()) {
        // Get user-facing parameters (exclude framework injection params)
        const userParams = this.extractUserFacingParams(commandName);

        const signature: CommandSignature = {
          name: commandName,
          description: userParams.description || `${commandName} command`,
          params: userParams.params,
          returns: {
            success: { type: 'boolean', description: 'Operation success status' }
          }
        };

        commandSignatures.push(signature);
      }

      return createListResultFromParams(listParams, {
        success: true,
        commands: commandSignatures,
        totalCount: commandSignatures.length
      });

    } catch (error) {
      console.error(`❌ SERVER: Failed to list commands:`, error);

      return createListResultFromParams(listParams, {
        success: false,
        commands: [],
        totalCount: 0,
        error: error instanceof Error ? error.message : 'Unknown error'
      });
    }
  }

  /**
   * Extract user-facing parameters for a command from generated schemas
   *
   * Framework params that are auto-injected and should NOT be exposed to tools:
   * - context (JTAGContext)
   * - sessionId (UUID)
   * - backend (JTAGEnvironment)
   */
  private extractUserFacingParams(commandName: string): {
    description: string;
    params: Record<string, { type: string; required: boolean; description?: string }>;
  } {
    // Try to find command in generated schemas
    if (ListServerCommand.generatedSchemas) {
      const schema = ListServerCommand.generatedSchemas.commands.find(cmd => cmd.name === commandName);

      if (schema) {
        // Filter out framework injection params
        const userParams: Record<string, { type: string; required: boolean; description?: string }> = {};
        const frameworkParams = ['context', 'sessionId', 'backend', 'contextId'];

        for (const [paramName, paramDef] of Object.entries(schema.params)) {
          if (!frameworkParams.includes(paramName)) {
            userParams[paramName] = paramDef;
          }
        }

        return {
          description: schema.description,
          params: userParams
        };
      }
    }

    // Fallback for commands not in generated schemas (or if schemas failed to load)
    return {
      description: `${commandName} command`,
      params: {}
    };
  }
}