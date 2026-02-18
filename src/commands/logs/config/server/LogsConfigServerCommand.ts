/**
 * Logs Config Command - Server Implementation
 *
 * Get or set logging configuration per persona and category
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { LogsConfigParams, LogsConfigResult, PersonaLoggingStatus } from '../shared/LogsConfigTypes';
import { createLogsConfigResultFromParams } from '../shared/LogsConfigTypes';
import { LoggingConfig, LOGGING_CATEGORIES, type LoggingConfigData } from '../../../../system/core/logging/LoggingConfig';
import { Commands } from '../../../../system/core/shared/Commands';

export class LogsConfigServerCommand extends CommandBase<LogsConfigParams, LogsConfigResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Logs Config', context, subpath, commander);
  }

  async execute(params: LogsConfigParams): Promise<LogsConfigResult> {
    const action = params.action || 'get';
    const persona = params.persona;
    const category = params.category;

    // Get current config
    const config = LoggingConfig.getConfig();
    const availableCategories = Object.values(LOGGING_CATEGORIES);

    // If just getting config (no action or action=get)
    if (action === 'get') {
      // If persona specified, return their config
      if (persona) {
        const personaConfig = config.personas[persona] || {
          enabled: config.defaults.enabled,
          categories: config.defaults.categories || []
        };
        return createLogsConfigResultFromParams(params, {
          success: true,
          config: config,
          availableCategories,
          personaConfig: {
            enabled: personaConfig.enabled,
            categories: personaConfig.categories || []
          },
          message: `Logging config for '${persona}': ${personaConfig.enabled ? 'ON' : 'OFF'}` +
            (personaConfig.enabled && personaConfig.categories?.length
              ? ` (${personaConfig.categories.join(', ')})`
              : personaConfig.enabled ? ' (all categories)' : '')
        });
      }

      // Build status list for all known personas
      const statuses = await this.buildPersonaStatuses(config);

      const enabledCount = statuses.filter(s => s.enabled).length;
      const lines: string[] = [
        `Default: ${config.defaults.enabled ? 'ON' : 'OFF'}`,
        `Personas: ${enabledCount}/${statuses.length} logging enabled`,
        `Categories: ${availableCategories.join(', ')}`,
      ];

      return createLogsConfigResultFromParams(params, {
        success: true,
        config: config,
        statuses,
        availableCategories,
        personaConfig: { enabled: false, categories: [] },
        message: lines.join(' | ')
      });
    }

    // Enable/disable requires persona
    if (!persona) {
      return createLogsConfigResultFromParams(params, {
        success: false,
        config: config,
        personaConfig: { enabled: false, categories: [] },
        message: `The '${action}' action requires --persona parameter`
      });
    }

    const enabled = action === 'enable';

    // If category specified, toggle just that category
    if (category) {
      LoggingConfig.setEnabled(persona, category, enabled);
      const updatedConfig = LoggingConfig.getConfig();
      const personaConfig = updatedConfig.personas[persona] || { enabled: false, categories: [] };

      return createLogsConfigResultFromParams(params, {
        success: true,
        config: updatedConfig,
        personaConfig: {
          enabled: personaConfig.enabled,
          categories: personaConfig.categories || []
        },
        message: `${enabled ? 'Enabled' : 'Disabled'} logging for '${persona}' category '${category}'`
      });
    }

    // Toggle entire persona
    LoggingConfig.setPersonaEnabled(persona, enabled);
    const updatedConfig = LoggingConfig.getConfig();
    const personaConfig = updatedConfig.personas[persona] || { enabled: false, categories: [] };

    return createLogsConfigResultFromParams(params, {
      success: true,
      config: updatedConfig,
      personaConfig: {
        enabled: personaConfig.enabled,
        categories: personaConfig.categories || []
      },
      message: `${enabled ? 'Enabled' : 'Disabled'} all logging for '${persona}'`
    });
  }

  /**
   * Build logging status for all known personas.
   * Queries the user database for AI personas, then cross-references
   * with the logging config to show each persona's current state.
   */
  private async buildPersonaStatuses(config: LoggingConfigData): Promise<PersonaLoggingStatus[]> {
    const statuses: PersonaLoggingStatus[] = [];

    // Query all users to get the full persona list
    const result = await Commands.execute<any, any>('data/list', {
      collection: 'users',
      limit: 100
    });

    if (result.success && result.items) {
      for (const user of result.items) {
        // Skip human users — only show AI personas
        const userType = (user.userType || user.type || '').toLowerCase();
        if (['human', 'owner', 'admin', 'user'].includes(userType)) continue;

        const uniqueId = user.uniqueId || '';
        if (!uniqueId) continue;

        const personaConfig = config.personas[uniqueId];
        statuses.push({
          persona: uniqueId,
          enabled: personaConfig?.enabled ?? config.defaults.enabled,
          categories: personaConfig?.categories ?? config.defaults.categories ?? [],
          source: personaConfig ? 'explicit' : 'default'
        });
      }
    }

    // Include explicitly configured personas not found in user list
    // (e.g., config entries for personas that haven't been seeded yet)
    for (const [personaId, personaConfig] of Object.entries(config.personas)) {
      if (personaId === '*') continue;
      if (statuses.some(s => s.persona === personaId)) continue;
      statuses.push({
        persona: personaId,
        enabled: personaConfig.enabled,
        categories: personaConfig.categories || [],
        source: 'explicit'
      });
    }

    // Sort: enabled first, then alphabetical
    statuses.sort((a, b) => {
      if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
      return a.persona.localeCompare(b.persona);
    });

    return statuses;
  }
}
