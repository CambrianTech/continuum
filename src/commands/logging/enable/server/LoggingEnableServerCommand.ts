/**
 * Logging Enable Command - Server Implementation
 *
 * Enable logging for a persona. Persists to .continuum/logging.json
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { LoggingEnableParams, LoggingEnableResult } from '../shared/LoggingEnableTypes';
import { createLoggingEnableResultFromParams } from '../shared/LoggingEnableTypes';
import { LoggingConfig, LOGGING_CATEGORIES } from '@system/core/logging/LoggingConfig';

export class LoggingEnableServerCommand extends CommandBase<LoggingEnableParams, LoggingEnableResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logging/enable', context, subpath, commander);
  }

  async execute(params: LoggingEnableParams): Promise<LoggingEnableResult> {
    // Validate persona parameter
    if (!params.persona || params.persona.trim() === '') {
      throw new ValidationError(
        'persona',
        `Missing required parameter 'persona'. ` +
        `Use: ./jtag logging/enable --persona=helper [--category=cognition]`
      );
    }

    const persona = params.persona.trim();
    const category = params.category?.trim();

    // Enable logging
    if (category) {
      // Enable specific category
      LoggingConfig.setEnabled(persona, category, true);
    } else {
      // Enable all categories for persona
      LoggingConfig.setPersonaEnabled(persona, true);
    }

    // Get current state after update
    const config = LoggingConfig.getConfig();
    const personaConfig = config.personas[persona.toLowerCase().replace(/\s+ai$/i, '').replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '')];

    // Determine enabled categories
    let categories: string[];
    if (personaConfig && personaConfig.enabled) {
      if (!personaConfig.categories || personaConfig.categories.length === 0) {
        categories = Object.values(LOGGING_CATEGORIES);
      } else if (personaConfig.categories.includes('*')) {
        categories = Object.values(LOGGING_CATEGORIES);
      } else {
        categories = personaConfig.categories;
      }
    } else {
      categories = [];
    }

    const message = category
      ? `Enabled ${category} logging for ${persona}`
      : `Enabled all logging for ${persona}`;

    return createLoggingEnableResultFromParams(params, {
      success: true,
      persona,
      categories,
      message,
    });
  }
}
