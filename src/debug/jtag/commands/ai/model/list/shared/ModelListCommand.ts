/**
 * Model List Command - Shared Logic
 *
 * Enumerate available AI models with their capabilities
 * Like AVCaptureDevice.DiscoverySession.devices(for:position:)
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { CommandParams, CommandResult } from '../../../../../system/core/types/JTAGTypes';
import type { ModelListParams, ModelListResult, ModelInfo, ModelCapabilities } from './ModelListTypes';
import { LOCAL_MODELS } from '../../../../../system/shared/Constants';

export abstract class ModelListCommand extends CommandBase<CommandParams, CommandResult> {
  static readonly commandName = 'model/list';

  /**
   * Model catalog with known capabilities
   * TODO: Make this dynamic by querying providers
   */
  protected getModelCatalog(): ModelInfo[] {
    return [
      // Llama 3.2 family (Candle)
      {
        name: 'llama3.2:1b',
        displayName: 'Llama 3.2 1B',
        provider: 'candle',
        parameters: '1B',
        contextLength: 128000,
        supportsJSON: true,
        supportsToolCalling: false,
        supportsStreaming: true,
        estimatedLatency: 50,
        estimatedTokensPerSecond: 100,
        memoryRequirement: '1.3GB',
        recommendedFor: ['gating', 'classification', 'extraction'],
        available: true
      },
      {
        name: LOCAL_MODELS.DEFAULT,
        displayName: 'Llama 3.2 3B',
        provider: 'candle',
        parameters: '3B',
        contextLength: 128000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 150,
        estimatedTokensPerSecond: 50,
        memoryRequirement: '2.0GB',
        recommendedFor: ['chat', 'reasoning', 'creative'],
        available: true
      },
      {
        name: 'phi3:mini',
        displayName: 'Phi-3 Mini',
        provider: 'candle',
        parameters: '3.8B',
        contextLength: 128000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 180,
        estimatedTokensPerSecond: 45,
        memoryRequirement: '2.2GB',
        recommendedFor: ['chat', 'reasoning', 'code'],
        available: true
      },
      // OpenAI models
      {
        name: 'gpt-4',
        displayName: 'GPT-4',
        provider: 'openai',
        parameters: '~1.76T',
        contextLength: 8192,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 1000,
        estimatedTokensPerSecond: 40,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'code', 'chat', 'creative'],
        available: true, // Via external API
        needsDownload: false
      },
      {
        name: 'gpt-4-turbo',
        displayName: 'GPT-4 Turbo',
        provider: 'openai',
        parameters: '~1.76T',
        contextLength: 128000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 800,
        estimatedTokensPerSecond: 60,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'code', 'chat'],
        available: true,
        needsDownload: false
      },
      {
        name: 'gpt-4o',
        displayName: 'GPT-4o',
        provider: 'openai',
        parameters: 'Unknown',
        contextLength: 128000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 600,
        estimatedTokensPerSecond: 80,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'code', 'chat', 'creative'],
        available: true,
        needsDownload: false
      },
      {
        name: 'gpt-4o-mini',
        displayName: 'GPT-4o Mini',
        provider: 'openai',
        parameters: 'Unknown',
        contextLength: 128000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 400,
        estimatedTokensPerSecond: 100,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'code', 'chat'],
        available: true,
        needsDownload: false
      },
      {
        name: 'gpt-3.5-turbo',
        displayName: 'GPT-3.5 Turbo',
        provider: 'openai',
        parameters: '~175B',
        contextLength: 16385,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 300,
        estimatedTokensPerSecond: 120,
        memoryRequirement: 'Cloud',
        recommendedFor: ['chat', 'code'],
        available: true,
        needsDownload: false
      },
      // Anthropic models
      {
        name: 'claude-3-opus',
        displayName: 'Claude 3 Opus',
        provider: 'anthropic',
        parameters: 'Unknown',
        contextLength: 200000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 1200,
        estimatedTokensPerSecond: 50,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'creative', 'code', 'chat'],
        available: true,
        needsDownload: false
      },
      {
        name: 'claude-3-sonnet',
        displayName: 'Claude 3 Sonnet',
        provider: 'anthropic',
        parameters: 'Unknown',
        contextLength: 200000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 800,
        estimatedTokensPerSecond: 70,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'code', 'chat'],
        available: true,
        needsDownload: false
      },
      {
        name: 'claude-3-5-sonnet',
        displayName: 'Claude 3.5 Sonnet',
        provider: 'anthropic',
        parameters: 'Unknown',
        contextLength: 200000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 700,
        estimatedTokensPerSecond: 80,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'code', 'chat', 'creative'],
        available: true,
        needsDownload: false
      },
      {
        name: 'claude-3-haiku',
        displayName: 'Claude 3 Haiku',
        provider: 'anthropic',
        parameters: 'Unknown',
        contextLength: 200000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 400,
        estimatedTokensPerSecond: 100,
        memoryRequirement: 'Cloud',
        recommendedFor: ['chat', 'gating', 'classification'],
        available: true,
        needsDownload: false
      },
      // X.AI models
      {
        name: 'grok-3',  // Updated from grok-beta (deprecated 2025-09-15)
        displayName: 'Grok (Beta)',
        provider: 'xai',
        parameters: 'Unknown',
        contextLength: 131072,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 900,
        estimatedTokensPerSecond: 60,
        memoryRequirement: 'Cloud',
        recommendedFor: ['reasoning', 'chat', 'creative'],
        available: true,
        needsDownload: false
      },
      // DeepSeek models
      {
        name: 'deepseek-chat',
        displayName: 'DeepSeek Chat',
        provider: 'deepseek',
        parameters: '~236B',
        contextLength: 64000,
        supportsJSON: true,
        supportsToolCalling: true,
        supportsStreaming: true,
        estimatedLatency: 700,
        estimatedTokensPerSecond: 70,
        memoryRequirement: 'Cloud',
        recommendedFor: ['chat', 'reasoning', 'code'],
        available: true,
        needsDownload: false
      }
    ];
  }

  /**
   * Filter models by capabilities (like camera discovery filters)
   */
  protected filterByCapabilities(models: ModelInfo[], capabilities?: ModelCapabilities): ModelInfo[] {
    if (!capabilities) {
      return models;
    }

    return models.filter(model => {
      // Parameter size constraints
      if (capabilities.maxParameters) {
        const maxParams = this.parseParameterSize(capabilities.maxParameters);
        const modelParams = this.parseParameterSize(model.parameters);
        if (modelParams > maxParams) return false;
      }

      if (capabilities.minParameters) {
        const minParams = this.parseParameterSize(capabilities.minParameters);
        const modelParams = this.parseParameterSize(model.parameters);
        if (modelParams < minParams) return false;
      }

      // Performance constraints
      if (capabilities.maxLatency && model.estimatedLatency > capabilities.maxLatency) {
        return false;
      }

      if (capabilities.minTokensPerSecond && model.estimatedTokensPerSecond < capabilities.minTokensPerSecond) {
        return false;
      }

      // Feature requirements
      if (capabilities.supportsJSON && !model.supportsJSON) return false;
      if (capabilities.supportsToolCalling && !model.supportsToolCalling) return false;
      if (capabilities.supportsStreaming && !model.supportsStreaming) return false;

      // Context requirements
      if (capabilities.minContextLength && model.contextLength < capabilities.minContextLength) {
        return false;
      }

      if (capabilities.maxContextLength && model.contextLength > capabilities.maxContextLength) {
        return false;
      }

      // Task type hints
      if (capabilities.taskType && !model.recommendedFor.includes(capabilities.taskType)) {
        return false;
      }

      // Provider constraints
      if (capabilities.preferredProviders && !capabilities.preferredProviders.includes(model.provider)) {
        return false;
      }

      if (capabilities.excludeProviders && capabilities.excludeProviders.includes(model.provider)) {
        return false;
      }

      return true;
    });
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

  /**
   * Sort models by preference (smallest/fastest first for gating, largest/best for reasoning)
   */
  protected sortModelsByPreference(models: ModelInfo[], taskType?: string): ModelInfo[] {
    return models.sort((a, b) => {
      // For gating/classification tasks, prefer smaller/faster models
      if (taskType === 'gating' || taskType === 'classification' || taskType === 'extraction') {
        return a.estimatedLatency - b.estimatedLatency;
      }

      // For other tasks, prefer larger models (better quality)
      return this.parseParameterSize(b.parameters) - this.parseParameterSize(a.parameters);
    });
  }
}
