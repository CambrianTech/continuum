/**
 * Model Search Command - Server Implementation
 *
 * Search HuggingFace for base models. Wraps the HF API with
 * Continuum-relevant filtering (size, architecture, coding capability).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ModelSearchParams, ModelSearchResult } from '../shared/ModelSearchTypes';
import { createModelSearchResultFromParams } from '../shared/ModelSearchTypes';

interface HFModel {
  id: string;
  author?: string;
  downloads?: number;
  likes?: number;
  tags?: string[];
  pipeline_tag?: string;
  lastModified?: string;
}

export class ModelSearchServerCommand extends CommandBase<ModelSearchParams, ModelSearchResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/search', context, subpath, commander);
  }

  async execute(params: ModelSearchParams): Promise<ModelSearchResult> {
    if (!params.query) {
      throw new ValidationError('query', 'Required: search query (e.g., "Qwen3.5", "codellama 34b")');
    }

    const limit = params.limit ?? 10;
    const sort = params.sort ?? 'downloads';

    console.log(`🔍 Model Search: "${params.query}" (limit=${limit}, sort=${sort})`);

    const searchParams = new URLSearchParams({
      search: params.query,
      sort: sort === 'recent' ? 'lastModified' : sort,
      direction: '-1',
      limit: String(limit * 2), // Over-fetch for filtering
    });

    const url = `https://huggingface.co/api/models?${searchParams.toString()}`;

    try {
      const response = await fetch(url, { headers: { 'Accept': 'application/json' } });
      if (!response.ok) {
        throw new Error(`HuggingFace API: ${response.status}`);
      }

      const hfModels: HFModel[] = await response.json();

      // Filter by size if requested
      let filtered = hfModels;
      if (params.minSize || params.maxSize) {
        filtered = hfModels.filter(m => {
          const sizeMatch = m.id.match(/(\d+)[bB]/);
          if (!sizeMatch) return true; // Can't determine size, include
          const sizeB = parseInt(sizeMatch[1]);
          if (params.minSize && sizeB < (params.minSize as number)) return false;
          if (params.maxSize && sizeB > (params.maxSize as number)) return false;
          return true;
        });
      }

      const models = filtered.slice(0, limit).map(m => ({
        id: m.id,
        author: m.author ?? '',
        downloads: m.downloads ?? 0,
        likes: m.likes ?? 0,
        tags: m.tags ?? [],
        pipelineTag: m.pipeline_tag ?? '',
        lastModified: m.lastModified ?? '',
      }));

      console.log(`   Found ${models.length} models`);

      return createModelSearchResultFromParams(params, {
        success: true,
        models,
        totalCount: models.length,
      });
    } catch (e) {
      throw new Error(`Model search failed: ${e instanceof Error ? e.message : e}`);
    }
  }
}
