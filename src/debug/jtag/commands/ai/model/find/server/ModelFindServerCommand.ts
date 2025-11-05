/**
 * Model Find Server Command
 *
 * Server-side implementation - uses model/list then picks best match
 */

import { ModelFindCommand } from '../shared/ModelFindCommand';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { ModelFindParams, ModelFindResult } from '../shared/ModelFindTypes';
import type { ModelListParams, ModelListResult } from '../../list/shared/ModelListTypes';
import type { ModelInfo } from '../../list/shared/ModelListTypes';
import { Commands } from '../../../../../system/core/shared/Commands';

export class ModelFindServerCommand extends ModelFindCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('ai/model/find', context, subpath, commander);
  }

  async execute(params: ModelFindParams): Promise<ModelFindResult> {
    try {
      // Use ai/model/list to get filtered candidates
      const listParams: ModelListParams = {
        ...params,
        capabilities: params.capabilities,
        includeUnavailable: false // Only available models
      };

      const listResult = await Commands.execute<ModelListParams, ModelListResult>(
        'ai/model/list',
        listParams
      );

      if (!listResult.success || listResult.models.length === 0) {
        // No models found
        if (params.allowFallback !== false) {
          // Try again without strict filtering
          console.log('🔍 MODEL FIND: No strict matches, trying fallback...');
          const fallbackParams = { ...listParams, capabilities: undefined };
          const fallbackResult = await Commands.execute<ModelListParams, ModelListResult>(
            'ai/model/list',
            fallbackParams
          );

          if (fallbackResult.success && fallbackResult.models.length > 0) {
            const model = fallbackResult.models[0]; // Take first available
            return {
              ...params,
              success: true,
              model,
              alternates: fallbackResult.models.slice(1, 4),
              matchScore: 50, // Low score since we had to fallback
              fallbackUsed: true
            };
          }
        }

        return {
          ...params,
          success: false,
          error: 'No models found matching requirements'
        };
      }

      // Score all candidates
      const scored = listResult.models.map((model: ModelInfo) => ({
        model,
        score: this.calculateMatchScore(model, params)
      }));

      // Sort by score (highest first)
      scored.sort((a: { model: ModelInfo; score: number }, b: { model: ModelInfo; score: number }) => b.score - a.score);

      // Return best match
      const best = scored[0];
      const alternates = scored.slice(1, 4).map((s: { model: ModelInfo; score: number }) => s.model);

      console.log(`✅ MODEL FIND: Selected ${best.model.name} (score: ${best.score.toFixed(1)})`);

      return {
        ...params,
        success: true,
        model: best.model,
        alternates,
        matchScore: best.score,
        fallbackUsed: false
      };

    } catch (error) {
      console.error('❌ MODEL FIND: Command failed:', error);
      return {
        ...params,
        success: false,
        error: error instanceof Error ? error.message : String(error)
      };
    }
  }
}
