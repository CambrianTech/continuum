/**
 * Adapter Search Command - Server Implementation
 *
 * Search for LoRA adapters across registries (HuggingFace, local, mesh)
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AdapterSearchParams, AdapterSearchResult, AdapterSearchResultItem, AdapterSource } from '../shared/AdapterSearchTypes';
import { createAdapterSearchResultFromParams } from '../shared/AdapterSearchTypes';
import * as fs from 'fs';
import * as path from 'path';

/**
 * HuggingFace API response types
 */
interface HFModelResult {
  _id: string;
  id: string;
  modelId: string;
  author?: string;
  sha?: string;
  lastModified?: string;
  private?: boolean;
  disabled?: boolean;
  downloads?: number;
  likes?: number;
  tags?: string[];
  pipeline_tag?: string;
  library_name?: string;
  cardData?: {
    base_model?: string | string[];
    language?: string[];
    license?: string;
    tags?: string[];
  };
}

export class AdapterSearchServerCommand extends CommandBase<AdapterSearchParams, AdapterSearchResult> {
  private readonly HF_API_BASE = 'https://huggingface.co/api/models';
  private readonly LOCAL_REGISTRY_PATH: string;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/search', context, subpath, commander);
    this.LOCAL_REGISTRY_PATH = path.join(
      process.env.HOME || '',
      '.continuum/adapters/installed'
    );
  }

  async execute(params: AdapterSearchParams): Promise<AdapterSearchResult> {
    const startTime = Date.now();
    console.log('🔍 Adapter Search:', params.query);

    // Validate required parameters
    if (!params.query || params.query.trim() === '') {
      throw new ValidationError(
        'query',
        `Missing required parameter 'query'. ` +
        `Use --help or see the adapter/search README for usage information.`
      );
    }

    const source = params.source || 'all';
    const limit = params.limit || 10;
    const sort = params.sort || 'downloads';
    const baseModelFilter = params.baseModel?.toLowerCase();

    const sourcesToSearch: AdapterSource[] = source === 'all'
      ? ['huggingface', 'local']  // mesh not implemented yet
      : [source];

    const allResults: AdapterSearchResultItem[] = [];

    // Search each source in parallel
    const searchPromises: Promise<AdapterSearchResultItem[]>[] = [];

    if (sourcesToSearch.includes('huggingface')) {
      searchPromises.push(this.searchHuggingFace(params.query, limit * 2, sort, baseModelFilter));
    }
    if (sourcesToSearch.includes('local')) {
      searchPromises.push(this.searchLocalRegistry(params.query, baseModelFilter));
    }
    if (sourcesToSearch.includes('mesh')) {
      // TODO: Implement mesh search when available
      searchPromises.push(Promise.resolve([]));
    }

    const resultsArrays = await Promise.all(searchPromises);
    for (const results of resultsArrays) {
      allResults.push(...results);
    }

    // Mark installed adapters
    const installedIds = await this.getInstalledAdapterIds();
    for (const result of allResults) {
      if (installedIds.has(result.id)) {
        result.installed = true;
        result.localPath = path.join(this.LOCAL_REGISTRY_PATH, result.id.replace('/', '--'));
      }
    }

    // Sort results
    this.sortResults(allResults, sort);

    // Apply limit
    const limitedResults = allResults.slice(0, limit);

    const searchTimeMs = Date.now() - startTime;
    console.log(`   Found ${allResults.length} adapters in ${searchTimeMs}ms`);

    return createAdapterSearchResultFromParams(params, {
      success: true,
      results: limitedResults,
      totalCount: allResults.length,
      query: params.query,
      searchTimeMs,
      sourcesSearched: sourcesToSearch,
    });
  }

  /**
   * Search HuggingFace Hub for LoRA adapters
   */
  private async searchHuggingFace(
    query: string,
    limit: number,
    sort: string,
    baseModelFilter?: string
  ): Promise<AdapterSearchResultItem[]> {
    try {
      // Build search URL with filters
      const searchParams = new URLSearchParams({
        search: query,
        filter: 'peft',  // PEFT library = LoRA adapters
        sort: sort === 'recent' ? 'lastModified' : sort,
        direction: '-1',  // Descending
        limit: String(limit),
      });

      const url = `${this.HF_API_BASE}?${searchParams.toString()}`;
      console.log(`   Searching HuggingFace: ${url}`);

      const response = await fetch(url, {
        headers: {
          'Accept': 'application/json',
        },
      });

      if (!response.ok) {
        console.error(`   HuggingFace API error: ${response.status}`);
        return [];
      }

      const models: HFModelResult[] = await response.json();
      console.log(`   HuggingFace returned ${models.length} results`);

      // Transform to our format
      const results: AdapterSearchResultItem[] = [];
      for (const model of models) {
        // Extract base model from cardData or tags
        let baseModel = '';
        if (model.cardData?.base_model) {
          baseModel = Array.isArray(model.cardData.base_model)
            ? model.cardData.base_model[0]
            : model.cardData.base_model;
        } else if (model.tags) {
          // Fallback: extract from tags like "base_model:meta-llama/Llama-3.2-1B-Instruct"
          const baseModelTag = model.tags.find(t => t.startsWith('base_model:') && !t.includes('adapter:'));
          if (baseModelTag) {
            baseModel = baseModelTag.replace('base_model:', '');
          }
        }

        // Apply base model filter if specified
        if (baseModelFilter && !baseModel.toLowerCase().includes(baseModelFilter)) {
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
          tags: model.tags,
          installed: false,
        });
      }

      return results;
    } catch (error) {
      console.error('   HuggingFace search failed:', error);
      return [];
    }
  }

  /**
   * Search local adapter registry
   */
  private async searchLocalRegistry(
    query: string,
    baseModelFilter?: string
  ): Promise<AdapterSearchResultItem[]> {
    const results: AdapterSearchResultItem[] = [];
    const queryLower = query.toLowerCase();

    try {
      if (!fs.existsSync(this.LOCAL_REGISTRY_PATH)) {
        return results;
      }

      const entries = fs.readdirSync(this.LOCAL_REGISTRY_PATH);
      for (const entry of entries) {
        const manifestPath = path.join(this.LOCAL_REGISTRY_PATH, entry, 'manifest.json');
        if (!fs.existsSync(manifestPath)) {
          continue;
        }

        try {
          const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));

          // Handle both snake_case (from Rust) and camelCase field names
          const repoId = manifest.repo_id || manifest.repoId;
          const baseModel = manifest.base_model || manifest.baseModel;

          // Check if matches query
          const matchesQuery =
            entry.toLowerCase().includes(queryLower) ||
            repoId?.toLowerCase().includes(queryLower) ||
            manifest.description?.toLowerCase().includes(queryLower);

          if (!matchesQuery) {
            continue;
          }

          // Apply base model filter
          if (baseModelFilter && !baseModel?.toLowerCase().includes(baseModelFilter)) {
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
            localPath: path.join(this.LOCAL_REGISTRY_PATH, entry),
          });
        } catch (e) {
          console.warn(`   Failed to read manifest for ${entry}:`, e);
        }
      }
    } catch (error) {
      console.error('   Local registry search failed:', error);
    }

    return results;
  }

  /**
   * Get set of installed adapter IDs for marking HF results
   */
  private async getInstalledAdapterIds(): Promise<Set<string>> {
    const ids = new Set<string>();
    try {
      if (!fs.existsSync(this.LOCAL_REGISTRY_PATH)) {
        return ids;
      }

      const entries = fs.readdirSync(this.LOCAL_REGISTRY_PATH);
      for (const entry of entries) {
        const manifestPath = path.join(this.LOCAL_REGISTRY_PATH, entry, 'manifest.json');
        if (fs.existsSync(manifestPath)) {
          try {
            const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf-8'));
            // Handle both snake_case and camelCase
            const repoId = manifest.repo_id || manifest.repoId;
            if (repoId) {
              ids.add(repoId);
            }
          } catch (e) {
            // Skip invalid manifests
          }
        }
      }
    } catch (error) {
      // Ignore errors
    }
    return ids;
  }

  /**
   * Extract description from HuggingFace model
   */
  private extractDescription(model: HFModelResult): string {
    const parts: string[] = [];

    if (model.pipeline_tag) {
      parts.push(`Task: ${model.pipeline_tag}`);
    }
    if (model.cardData?.language) {
      parts.push(`Language: ${model.cardData.language.join(', ')}`);
    }
    if (model.cardData?.license) {
      parts.push(`License: ${model.cardData.license}`);
    }

    return parts.length > 0 ? parts.join(' | ') : `LoRA adapter by ${model.author || 'unknown'}`;
  }

  /**
   * Sort results based on criteria
   */
  private sortResults(results: AdapterSearchResultItem[], sort: string): void {
    switch (sort) {
      case 'downloads':
        results.sort((a, b) => (b.downloads || 0) - (a.downloads || 0));
        break;
      case 'likes':
        results.sort((a, b) => (b.likes || 0) - (a.likes || 0));
        break;
      case 'recent':
        results.sort((a, b) => {
          const dateA = a.lastModified ? new Date(a.lastModified).getTime() : 0;
          const dateB = b.lastModified ? new Date(b.lastModified).getTime() : 0;
          return dateB - dateA;
        });
        break;
    }
  }
}
