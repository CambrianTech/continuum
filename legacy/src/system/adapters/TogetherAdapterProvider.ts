/**
 * Together.ai Adapter Provider
 *
 * Cloud LoRA hosting via Together.ai's Serverless Multi-LoRA platform.
 * Upload adapters from HuggingFace, pay base model per-token pricing.
 *
 * Docs: https://docs.together.ai/docs/lora-inference
 */

import type {
  IAdapterProvider,
  ProviderType,
  SupportedModel,
  AdapterSearchOptions,
  CompatibilityResult,
  DeployedAdapter,
  CostEstimate,
} from './IAdapterProvider';
import type { AdapterSearchResultItem, AdapterSource } from '../../commands/adapter/search/shared/AdapterSearchTypes';

/**
 * Together.ai adapter provider
 */
export class TogetherAdapterProvider implements IAdapterProvider {
  readonly name = 'together';
  readonly type: ProviderType = 'cloud-lora';
  readonly source: AdapterSource = 'huggingface';  // Sources from HF
  readonly description = 'Together.ai Serverless Multi-LoRA - cloud inference with HuggingFace adapters';

  private readonly apiBase = 'https://api.together.xyz/v1';
  private apiKey: string | undefined;

  constructor(apiKey?: string) {
    this.apiKey = apiKey || process.env.TOGETHER_API_KEY;
  }

  /**
   * Models supported by Together.ai for LoRA
   * https://docs.together.ai/docs/lora-inference#supported-base-models
   */
  async getSupportedModels(): Promise<SupportedModel[]> {
    return [
      {
        id: 'meta-llama/Llama-3.1-8B-Instruct',
        name: 'Llama 3.1 8B Instruct',
        family: 'llama',
        maxContext: 131072,
        supportedRanks: [8, 16, 32, 64, 128],
      },
      {
        id: 'meta-llama/Llama-3.1-70B-Instruct',
        name: 'Llama 3.1 70B Instruct',
        family: 'llama',
        maxContext: 131072,
        supportedRanks: [8, 16, 32, 64],
      },
      {
        id: 'Qwen/Qwen2.5-7B-Instruct',
        name: 'Qwen 2.5 7B Instruct',
        family: 'qwen',
        maxContext: 32768,
        supportedRanks: [8, 16, 32, 64],
      },
      {
        id: 'Qwen/Qwen2.5-72B-Instruct',
        name: 'Qwen 2.5 72B Instruct',
        family: 'qwen',
        maxContext: 32768,
        supportedRanks: [8, 16, 32],
      },
      {
        id: 'deepseek-ai/DeepSeek-R1-Distill-Llama-70B',
        name: 'DeepSeek R1 Distill 70B',
        family: 'deepseek',
        maxContext: 32768,
        supportedRanks: [8, 16, 32],
      },
      {
        id: 'mistralai/Mixtral-8x7B-Instruct-v0.1',
        name: 'Mixtral 8x7B Instruct',
        family: 'mixtral',
        maxContext: 32768,
        supportedRanks: [8, 16, 32],
      },
    ];
  }

  /**
   * Search HuggingFace for adapters compatible with Together.ai
   */
  async search(options: AdapterSearchOptions): Promise<AdapterSearchResultItem[]> {
    // Search HuggingFace API
    const searchParams = new URLSearchParams({
      search: options.query,
      filter: 'peft',
      sort: options.sort === 'recent' ? 'lastModified' : (options.sort || 'downloads'),
      direction: '-1',
      limit: String((options.limit || 10) * 2),  // Get extra for filtering
    });

    const url = `https://huggingface.co/api/models?${searchParams.toString()}`;
    const response = await fetch(url);
    if (!response.ok) return [];

    const models = await response.json();
    const supportedModels = await this.getSupportedModels();
    const supportedFamilies = new Set(supportedModels.map(m => m.family));

    const results: AdapterSearchResultItem[] = [];
    for (const model of models) {
      // Extract base model from tags
      let baseModel = '';
      if (model.tags) {
        const baseModelTag = model.tags.find((t: string) =>
          t.startsWith('base_model:') && !t.includes('adapter:')
        );
        if (baseModelTag) {
          baseModel = baseModelTag.replace('base_model:', '');
        }
      }

      // Filter by base model compatibility with Together.ai
      const baseModelLower = baseModel.toLowerCase();
      const isCompatible = Array.from(supportedFamilies).some(f =>
        baseModelLower.includes(f)
      );

      // Apply user's base model filter
      if (options.baseModel && !baseModelLower.includes(options.baseModel.toLowerCase())) {
        continue;
      }

      results.push({
        id: model.id,
        name: model.modelId || model.id,
        description: this.extractDescription(model),
        baseModel,
        source: 'huggingface',
        downloads: model.downloads,
        likes: model.likes,
        lastModified: model.lastModified,
        author: model.author,
        tags: [
          ...(model.tags || []),
          isCompatible ? 'together-compatible' : 'together-incompatible',
        ],
        installed: false,
      });
    }

    return results.slice(0, options.limit || 10);
  }

  /**
   * Check if adapter is compatible with Together.ai
   */
  async checkCompatibility(
    adapter: AdapterSearchResultItem,
    targetModel?: string
  ): Promise<CompatibilityResult> {
    const supportedModels = await this.getSupportedModels();
    const adapterBase = adapter.baseModel.toLowerCase();

    // Check family compatibility
    const matchingModel = supportedModels.find(m =>
      adapterBase.includes(m.family) ||
      m.id.toLowerCase() === adapterBase ||
      adapterBase.includes(m.id.toLowerCase().split('/')[1])
    );

    if (!matchingModel) {
      return {
        compatible: false,
        reason: `Base model "${adapter.baseModel}" not supported by Together.ai. Supported: ${supportedModels.map(m => m.name).join(', ')}`,
        suggestedProvider: 'fireworks',  // Try alternative cloud provider
      };
    }

    // Check rank compatibility
    if (adapter.rank && !matchingModel.supportedRanks.includes(adapter.rank)) {
      return {
        compatible: false,
        reason: `Rank ${adapter.rank} not supported. Together.ai supports ranks: ${matchingModel.supportedRanks.join(', ')}`,
      };
    }

    return {
      compatible: true,
    };
  }

  /**
   * Deploy adapter to Together.ai
   * Uploads from HuggingFace to Together's registry
   */
  async deploy(
    adapterId: string,
    options?: { baseModel?: string; scale?: number; alias?: string }
  ): Promise<DeployedAdapter> {
    if (!this.apiKey) {
      return {
        adapterId,
        provider: 'together',
        status: 'failed',
        error: 'TOGETHER_API_KEY not configured',
      };
    }

    try {
      // Upload adapter to Together.ai from HuggingFace
      const response = await fetch(`${this.apiBase}/models`, {
        method: 'POST',
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          model_name: options?.alias || adapterId.replace('/', '-'),
          model_source: 'huggingface',
          model_type: 'lora',
          hf_model_name: adapterId,
          base_model: options?.baseModel,
          description: `Uploaded from HuggingFace: ${adapterId}`,
        }),
      });

      if (!response.ok) {
        const error = await response.text();
        return {
          adapterId,
          provider: 'together',
          status: 'failed',
          error: `Upload failed: ${error}`,
        };
      }

      const result = await response.json();
      return {
        adapterId,
        provider: 'together',
        endpoint: result.endpoint || `together/${options?.alias || adapterId}`,
        status: 'ready',
      };
    } catch (error) {
      return {
        adapterId,
        provider: 'together',
        status: 'failed',
        error: String(error),
      };
    }
  }

  /**
   * Remove adapter from Together.ai
   */
  async undeploy(adapterId: string): Promise<void> {
    if (!this.apiKey) return;

    await fetch(`${this.apiBase}/models/${adapterId}`, {
      method: 'DELETE',
      headers: {
        'Authorization': `Bearer ${this.apiKey}`,
      },
    });
  }

  /**
   * List deployed adapters
   */
  async listDeployed(): Promise<DeployedAdapter[]> {
    if (!this.apiKey) return [];

    try {
      const response = await fetch(`${this.apiBase}/models`, {
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
        },
      });

      if (!response.ok) return [];

      const models = await response.json();
      return models
        .filter((m: any) => m.type === 'lora')
        .map((m: any) => ({
          adapterId: m.id,
          provider: 'together',
          endpoint: m.endpoint,
          status: m.status === 'ready' ? 'ready' : 'pending',
        }));
    } catch {
      return [];
    }
  }

  /**
   * Cost estimate based on Together.ai pricing
   * LoRA adapters use base model pricing
   */
  async estimateCost(adapterId: string, tokensPerMonth: number): Promise<CostEstimate> {
    // Together.ai pricing (as of 2025)
    // Llama 3.1 8B: $0.18/M input, $0.18/M output
    // Llama 3.1 70B: $0.88/M input, $0.88/M output
    // Using average estimate
    const perMillion = 0.20;  // Conservative estimate
    const monthlyTokensInMillions = tokensPerMonth / 1_000_000;

    return {
      provider: 'together',
      perMillionTokens: perMillion,
      monthlyEstimate: monthlyTokensInMillions * perMillion,
      notes: 'LoRA adapters use base model per-token pricing. No adapter overhead.',
    };
  }

  /**
   * Check if Together.ai is reachable
   */
  async ping(): Promise<boolean> {
    if (!this.apiKey) return false;

    try {
      const response = await fetch(`${this.apiBase}/models`, {
        headers: {
          'Authorization': `Bearer ${this.apiKey}`,
        },
      });
      return response.ok;
    } catch {
      return false;
    }
  }

  private extractDescription(model: any): string {
    const parts: string[] = [];
    if (model.pipeline_tag) parts.push(`Task: ${model.pipeline_tag}`);
    if (model.cardData?.license) parts.push(`License: ${model.cardData.license}`);
    return parts.length > 0 ? parts.join(' | ') : `LoRA adapter by ${model.author || 'unknown'}`;
  }
}
