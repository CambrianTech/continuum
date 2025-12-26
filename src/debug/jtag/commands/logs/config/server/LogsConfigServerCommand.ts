/**
 * Logs Config Command - Server Implementation
 *
 * Get or set logging configuration per persona and category
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { LogsConfigParams, LogsConfigResult } from '../shared/LogsConfigTypes';
import { createLogsConfigResultFromParams } from '../shared/LogsConfigTypes';
import { LoggingConfig, LOGGING_CATEGORIES } from '../../../../system/core/logging/LoggingConfig';

export class LogsConfigServerCommand extends CommandBase<LogsConfigParams, LogsConfigResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('Logs Config', context, subpath, commander);
  }

  async execute(params: LogsConfigParams): Promise<LogsConfigResult> {
    console.log('🔧 SERVER: Executing Logs Config', params);

    const action = params.action || 'get';
    const persona = params.persona;
    const category = params.category;

    // Get current config
    const config = LoggingConfig.getConfig();

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
          personaConfig: {
            enabled: personaConfig.enabled,
            categories: personaConfig.categories || []
          },
          message: `Logging config for '${persona}': ${personaConfig.enabled ? 'enabled' : 'disabled'}` +
            (personaConfig.categories?.length ? ` (categories: ${personaConfig.categories.join(', ')})` : ' (all categories)')
        });
      }

      // Return full config
      return createLogsConfigResultFromParams(params, {
        success: true,
        config: config,
        personaConfig: { enabled: false, categories: [] },
        message: `Available categories: ${Object.values(LOGGING_CATEGORIES).join(', ')}`
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
}
