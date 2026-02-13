/**
 * Logging Status Command - Server Implementation
 *
 * Show current logging configuration for all personas or a specific persona
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { LoggingStatusParams, LoggingStatusResult } from '../shared/LoggingStatusTypes';
import { createLoggingStatusResultFromParams } from '../shared/LoggingStatusTypes';
import { LoggingConfig, LOGGING_CATEGORIES } from '@system/core/logging/LoggingConfig';

interface PersonaStatus {
  persona: string;
  enabled: boolean;
  categories: string[];
}

export class LoggingStatusServerCommand extends CommandBase<LoggingStatusParams, LoggingStatusResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('logging/status', context, subpath, commander);
  }

  async execute(params: LoggingStatusParams): Promise<LoggingStatusResult> {
    const config = LoggingConfig.getConfig();
    const allCategories = Object.values(LOGGING_CATEGORIES);
    const personaFilter = params.persona?.trim().toLowerCase().replace(/\s+ai$/i, '').replace(/\s+/g, '-').replace(/[^a-z0-9-]/g, '');

    // Build persona status list
    const personas: PersonaStatus[] = [];

    for (const [personaId, personaConfig] of Object.entries(config.personas)) {
      // Skip if filtering for specific persona
      if (personaFilter && personaId !== personaFilter) {
        continue;
      }

      let categories: string[];
      if (!personaConfig.enabled) {
        categories = [];
      } else if (!personaConfig.categories || personaConfig.categories.length === 0) {
        categories = allCategories;
      } else if (personaConfig.categories.includes('*')) {
        categories = allCategories;
      } else {
        categories = personaConfig.categories;
      }

      personas.push({
        persona: personaId,
        enabled: personaConfig.enabled,
        categories,
      });
    }

    // System logging status
    const systemEnabled = config.system?.enabled ?? false;
    const defaultEnabled = config.defaults?.enabled ?? false;

    // Build summary
    const enabledCount = personas.filter(p => p.enabled).length;
    let summary: string;

    if (personaFilter) {
      const found = personas.find(p => p.persona === personaFilter);
      if (found) {
        summary = found.enabled
          ? `${found.persona}: logging ENABLED (${found.categories.length} categories)`
          : `${found.persona}: logging DISABLED`;
      } else {
        summary = `${personaFilter}: using defaults (${defaultEnabled ? 'enabled' : 'disabled'})`;
      }
    } else {
      summary = `${enabledCount} persona(s) with logging enabled. Default: ${defaultEnabled ? 'ON' : 'OFF'}. System: ${systemEnabled ? 'ON' : 'OFF'}`;
    }

    return createLoggingStatusResultFromParams(params, {
      success: true,
      personas,
      systemEnabled,
      defaultEnabled,
      availableCategories: allCategories,
      summary,
    });
  }
}
