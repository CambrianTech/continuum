/**
 * Model List Server Command
 *
 * Server-side implementation that queries actual model availability
 */

import { ModelListCommand } from '../shared/ModelListCommand';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { ModelListParams, ModelListResult } from '../shared/ModelListTypes';

export class ModelListServerCommand extends ModelListCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/model/list', context, subpath, commander);
  }

  async execute(params: ModelListParams): Promise<ModelListResult> {
    try {
      // Get model catalog
      let models = this.getModelCatalog();

      // Filter by provider if specified
      if (params.providerFilter) {
        models = models.filter(m => m.provider === params.providerFilter);
      }

      // Filter by availability
      if (!params.includeUnavailable) {
        models = models.filter(m => m.available);
      }

      // Filter by capabilities if specified
      if (params.capabilities) {
        models = this.filterByCapabilities(models, params.capabilities);
      }

      // Sort by preference
      models = this.sortModelsByPreference(models, params.capabilities?.taskType);

      // Get provider list
      const providers = [...new Set(models.map(m => m.provider))];

      return {
        ...params,
        success: true,
        models,
        totalCount: models.length,
        availableCount: models.filter(m => m.available).length,
        providers
      };

    } catch (error) {
      console.error('❌ MODEL LIST: Command failed:', error);
      return {
        ...params,
        success: false,
        error: error instanceof Error ? error.message : String(error),
        models: [],
        totalCount: 0,
        availableCount: 0,
        providers: []
      };
    }
  }
}
