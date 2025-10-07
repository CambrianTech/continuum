/**
 * Model Find Command - Shared Logic
 *
 * Find best AI model matching capability requirements
 * Like AVCaptureDevice.default() - intelligent device selection
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { CommandParams, CommandResult } from '../../../../system/core/types/JTAGTypes';
import type { ModelFindParams, ModelFindResult } from './ModelFindTypes';
import type { ModelInfo } from '../../list/shared/ModelListTypes';

export abstract class ModelFindCommand extends CommandBase<CommandParams, CommandResult> {
  static readonly commandName = 'model/find';

  /**
   * Calculate match score for a model against requirements (0-100)
   */
  protected calculateMatchScore(model: ModelInfo, params: ModelFindParams): number {
    let score = 100;
    const caps = params.capabilities;

    // Penalize if requirements not met (should have been filtered already, but double-check)
    if (caps.supportsJSON && !model.supportsJSON) score -= 50;
    if (caps.supportsToolCalling && !model.supportsToolCalling) score -= 30;
    if (caps.supportsStreaming && !model.supportsStreaming) score -= 10;

    // Reward based on selection strategy
    if (params.preferSmallest) {
      // Prefer smaller models - penalize larger ones
      const sizeMultiplier = this.parseParameterSize(model.parameters) / 1000000000; // Scale to billions
      score -= sizeMultiplier * 5; // -5 per billion parameters
    }

    if (params.preferLargest) {
      // Prefer larger models - reward larger ones
      const sizeMultiplier = this.parseParameterSize(model.parameters) / 1000000000;
      score += sizeMultiplier * 5;
    }

    if (params.preferFastest) {
      // Penalize based on latency
      score -= model.estimatedLatency / 10; // -1 per 10ms
    }

    // Reward if recommended for task type
    if (caps.taskType && model.recommendedFor.includes(caps.taskType)) {
      score += 20;
    }

    return Math.max(0, Math.min(100, score));
  }

  /**
   * Parse parameter size string to number (for comparison)
   */
  protected parseParameterSize(size: string): number {
    const match = size.match(/^(\d+(?:\.\d+)?)\s*([KMBT]?)B?$/i);
    if (!match) return 0;

    const num = parseFloat(match[1]);
    const unit = match[2].toUpperCase();

    const multipliers: Record<string, number> = {
      '': 1,
      'K': 1000,
      'M': 1000000,
      'B': 1000000000,
      'T': 1000000000000
    };

    return num * (multipliers[unit] || 1);
  }
}
