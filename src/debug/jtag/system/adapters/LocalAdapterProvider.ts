/**
 * Local Adapter Provider
 *
 * Manages LoRA adapters for local inference via Candle.
 * Direct weight merging - no cloud dependencies.
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
import { InferenceGrpcClient } from '../core/services/InferenceGrpcClient';
import * as fs from 'fs';
import * as path from 'path';

/**
 * Local adapter provider - Candle inference
 */
export class LocalAdapterProvider implements IAdapterProvider {
  readonly name = 'local';
  readonly type: ProviderType = 'local';
  readonly source: AdapterSource = 'local';
  readonly description = 'Local inference via Candle with direct LoRA weight merging';

  private readonly registryPath: string;
  private readonly client: InferenceGrpcClient;

  constructor() {
    this.registryPath = path.join(
      process.env.HOME || '',
      '.continuum/adapters/installed'
    );
    this.client = new InferenceGrpcClient();
  }

  /**
   * Models we can run locally (depends on hardware)
   * TODO: Detect from inference worker capabilities
   */
  async getSupportedModels(): Promise<SupportedModel[]> {
    return [
      {
        id: 'unsloth/Llama-3.2-3B-Instruct',
        name: 'Llama 3.2 3B',
        family: 'llama',
        maxContext: 8192,
        supportedRanks: [1, 2, 4, 8, 16, 32, 64],
      },
      {
        id: 'meta-llama/Llama-3.2-3B-Instruct',
        name: 'Llama 3.2 3B (Meta)',
        family: 'llama',
        maxContext: 8192,
        supportedRanks: [1, 2, 4, 8, 16, 32, 64],
      },
      {
        id: 'meta-llama/Llama-3.2-1B-Instruct',
        name: 'Llama 3.2 1B',
        family: 'llama',
        maxContext: 8192,
        supportedRanks: [1, 2, 4, 8, 16, 32],
      },
      // Add more as we support them
    ];
  }

  /**
   * Search local registry
   */
  async search(options: AdapterSearchOptions): Promise<AdapterSearchResultItem[]> {
    const results: AdapterSearchResultItem[] = [];
    const queryLower = options.query.toLowerCase();

    if (!fs.existsSync(this.registryPath)) {
      return results;
    }

    const entries = fs.readdirSync(this.registryPath);
    for (const entry of entries) {
      const manifestPath = path.join(this.registryPath, entry, 'manifest.json');
      if (!fs.existsSync(manifestPath)) continue;

      try {
        const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
        const repoId = manifest.repo_id || manifest.repoId;
        const baseModel = manifest.base_model || manifest.baseModel;

        // Match query
        const matchesQuery =
          entry.toLowerCase().includes(queryLower) ||
          repoId?.toLowerCase().includes(queryLower) ||
          manifest.description?.toLowerCase().includes(queryLower);

        if (!matchesQuery) continue;

        // Match base model filter
        if (options.baseModel && !baseModel?.toLowerCase().includes(options.baseModel.toLowerCase())) {
          continue;
        }

        const stats = fs.statSync(manifestPath);
        results.push({
          id: repoId || entry,
          name: repoId?.split('/').pop() || entry,
          description: manifest.description || `Local adapter: ${entry}`,
          baseModel: baseModel || '',
          source: 'local',
          lastModified: stats.mtime.toISOString(),
          rank: manifest.rank,
          installed: true,
          localPath: path.join(this.registryPath, entry),
        });
      } catch (e) {
        // Skip invalid manifests
      }
    }

    // Apply limit
    if (options.limit) {
      return results.slice(0, options.limit);
    }
    return results;
  }

  /**
   * Check if adapter is compatible with local inference
   */
  async checkCompatibility(
    adapter: AdapterSearchResultItem,
    targetModel?: string
  ): Promise<CompatibilityResult> {
    const supportedModels = await this.getSupportedModels();
    const adapterBase = adapter.baseModel.toLowerCase();

    // Check if base model is supported
    const matchingModel = supportedModels.find(m =>
      adapterBase.includes(m.family) ||
      m.id.toLowerCase() === adapterBase
    );

    if (!matchingModel) {
      return {
        compatible: false,
        reason: `Base model "${adapter.baseModel}" not supported locally. Supported families: ${supportedModels.map(m => m.family).join(', ')}`,
        suggestedProvider: 'together',  // Cloud providers support more models
      };
    }

    // Check rank if known
    if (adapter.rank && !matchingModel.supportedRanks.includes(adapter.rank)) {
      return {
        compatible: false,
        reason: `Rank ${adapter.rank} not optimal for local inference. Supported: ${matchingModel.supportedRanks.join(', ')}`,
      };
    }

    return { compatible: true };
  }

  /**
   * Deploy adapter - download and load into inference worker
   */
  async deploy(
    adapterId: string,
    options?: { baseModel?: string; scale?: number; alias?: string }
  ): Promise<DeployedAdapter> {
    try {
      // Check if already installed
      const localPath = path.join(this.registryPath, adapterId.replace('/', '--'));
      const isInstalled = fs.existsSync(path.join(localPath, 'manifest.json'));

      if (!isInstalled) {
        // Download from HuggingFace
        console.log(`📥 Downloading ${adapterId} from HuggingFace...`);
        const downloadResult = await this.client.downloadAdapter(adapterId, {
          adapterId: options?.alias || adapterId.replace('/', '--'),
          scale: options?.scale || 1.0,
        });

        if (!downloadResult.success) {
          return {
            adapterId,
            provider: 'local',
            status: 'failed',
            error: downloadResult.error,
          };
        }
      }

      // Load into inference worker
      console.log(`🔧 Loading ${adapterId} into inference worker...`);
      const loadResult = await this.client.loadAdapter(
        options?.alias || adapterId.replace('/', '--'),
        localPath,
        options?.scale || 1.0
      );

      if (!loadResult.success) {
        return {
          adapterId,
          provider: 'local',
          localPath,
          status: 'failed',
          error: loadResult.error,
        };
      }

      return {
        adapterId,
        provider: 'local',
        localPath,
        status: 'ready',
      };
    } catch (error) {
      return {
        adapterId,
        provider: 'local',
        status: 'failed',
        error: String(error),
      };
    }
  }

  /**
   * Unload adapter from inference worker
   */
  async undeploy(adapterId: string): Promise<void> {
    await this.client.unloadAdapter(adapterId);
  }

  /**
   * List currently loaded adapters
   */
  async listDeployed(): Promise<DeployedAdapter[]> {
    const adapters = await this.client.listAdapters();
    return adapters.map(a => ({
      adapterId: a.adapterId,
      provider: 'local',
      localPath: a.path,
      status: a.active ? 'ready' : 'pending',
    }));
  }

  /**
   * Cost estimate - local is free!
   */
  async estimateCost(adapterId: string, tokensPerMonth: number): Promise<CostEstimate> {
    return {
      provider: 'local',
      perMillionTokens: 0,
      monthlyEstimate: 0,
      notes: 'Free - uses local compute (electricity cost only)',
    };
  }

  /**
   * Check if inference worker is available
   */
  async ping(): Promise<boolean> {
    try {
      const result = await this.client.ping();
      return result.status === 'ok';
    } catch {
      return false;
    }
  }
}
