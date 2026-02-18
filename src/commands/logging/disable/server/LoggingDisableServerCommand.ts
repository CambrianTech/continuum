/**
 * Logging Disable Command - Server Implementation
 *
 * Disable logging for a persona. Persists to .continuum/logging.json
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { LoggingDisableParams, LoggingDisableResult } from '../shared/LoggingDisableTypes';
import { createLoggingDisableResultFromParams } from '../shared/LoggingDisableTypes';
import { LoggingConfig, LOGGING_CATEGORIES } from '@system/core/logging/LoggingConfig';

export class LoggingDisableServerCommand extends CommandBase<LoggingDisableParams, LoggingDisableResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logging/disable', context, subpath, commander);
  }

  async execute(params: LoggingDisableParams): Promise<LoggingDisableResult> {
    // Validate persona parameter
    if (!params.persona || params.persona.trim() === '') {
      throw new ValidationError(
        'persona',
        `Missing required parameter 'persona'. ` +
        `Use: ./jtag logging/disable --persona=helper [--category=cognition]`
      );
    }

    const persona = params.persona.trim();
    const category = params.category?.trim();

    // Disable logging
    if (category) {
      // Disable specific category
      LoggingConfig.setEnabled(persona, category, false);
    } else {
      // Disable all logging for persona
      LoggingConfig.setPersonaEnabled(persona, false);
    }

    // Get current state after update
    const config = LoggingConfig.getConfig();
    const normalizedId = persona.toLowerCase().replace(/\s+ai$/i, '').replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '');
    const personaConfig = config.personas[normalizedId];

    // Determine remaining enabled categories
    let categories: string[] = [];
    let enabled = false;

    if (personaConfig && personaConfig.enabled) {
      enabled = true;
      if (!personaConfig.categories || personaConfig.categories.length === 0) {
        categories = Object.values(LOGGING_CATEGORIES);
      } else if (personaConfig.categories.includes('*')) {
        categories = Object.values(LOGGING_CATEGORIES);
      } else {
        categories = personaConfig.categories;
      }
    }

    const message = category
      ? `Disabled ${category} logging for ${persona}`
      : `Disabled all logging for ${persona}`;

    return createLoggingDisableResultFromParams(params, {
      success: true,
      persona,
      enabled,
      categories,
      message,
    });
  }
}
