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
        ListServerCommand.schemasLoadError = 'Command parameters unavailable: generated-command-schemas.json not found. Commands will show without parameter documentation. Run: npx tsx generator/generate-command-schemas.ts';
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

    // Get commands from CommandDaemon
    const availableCommands = this.commander.commands;
    const commandSignatures: CommandSignature[] = [];

    // Convert CommandDaemon commands to CommandSignature format
    for (const [commandName, command] of availableCommands.entries()) {
      // Get command metadata from generated schemas
      const metadata = this.extractCommandMetadata(commandName);

      const signature: CommandSignature = {
        name: commandName,
        description: metadata.description,
        params: metadata.params,
        returns: metadata.returns
      };

      commandSignatures.push(signature);
    }

    return createListResultFromParams(listParams, {
      success: true,
      commands: commandSignatures,
      totalCount: commandSignatures.length
    });
  }

  /**
   * Extract complete command metadata from generated schemas
   *
   * Framework params that are auto-injected and should NOT be exposed:
   * - context (JTAGContext)
   * - sessionId (UUID)
   * - backend (JTAGEnvironment)
   * - contextId (UUID)
   */
  private extractCommandMetadata(commandName: string): {
    description: string;
    params: Record<string, { type: string; required: boolean; description?: string }>;
    returns: Record<string, { type: string; description?: string }>;
  } {
    // Try to find command in generated schemas
    if (ListServerCommand.generatedSchemas) {
      const schema = ListServerCommand.generatedSchemas.commands.find(
        cmd => cmd.name === commandName
      );

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
          params: userParams,
          returns: this.getReturnTypeForCommand(commandName)
        };
      }
    }

    // Fallback for commands not in generated schemas
    return {
      description: `${commandName} command`,
      params: {},
      returns: { success: { type: 'boolean', description: 'Operation success status' } }
    };
  }

  /**
   * Get return type for a command
   * TODO: Schema generator should extract return types from TypeScript CommandResult types
   * For now, return CommandResult base type
   */
  private getReturnTypeForCommand(commandName: string): Record<string, { type: string; description?: string }> {
    // All commands extend CommandResult which has these base properties
    return {
      success: { type: 'boolean', description: 'Whether command executed successfully' },
      error: { type: 'string', description: 'Error message if success=false' },
      timestamp: { type: 'string', description: 'ISO 8601 timestamp' },
      sessionId: { type: 'string', description: 'Session that executed command' }
    };
  }
}