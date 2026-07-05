/**
 * Model List Published Command - Server Implementation
 *
 * Fetches published model info from HuggingFace API.
 * Caches results for 5 minutes to avoid rate limiting.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ModelListPublishedParams, ModelListPublishedResult, PublishedModelInfo } from '../shared/ModelListPublishedTypes';
import { createModelListPublishedResultFromParams } from '../shared/ModelListPublishedTypes';

const HF_ORG = 'continuum-ai';
const CACHE_TTL_MS = 5 * 60 * 1000; // 5 minutes

let _cache: PublishedModelInfo[] | null = null;
let _cacheTime = 0;

export class ModelListPublishedServerCommand extends CommandBase<ModelListPublishedParams, ModelListPublishedResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/list-published', context, subpath, commander);
  }

  async execute(params: ModelListPublishedParams): Promise<ModelListPublishedResult> {
    let models = await this.fetchModels();

    // Filter by domain if specified
    if (params.domain) {
      models = models.filter(m => m.domain === params.domain);
    }

    // Filter out GGUF unless requested
    if (!params.includeGguf) {
      models = models.filter(m => m.variant !== 'gguf');
    }

    const totalDownloads = models.reduce((sum, m) => sum + m.downloads, 0);

    return createModelListPublishedResultFromParams(params, {
      success: true,
      models,
      totalDownloads,
      totalModels: models.length,
    });
  }

  private async fetchModels(): Promise<PublishedModelInfo[]> {
    if (_cache && Date.now() - _cacheTime < CACHE_TTL_MS) {
      return _cache;
    }

    try {
      const resp = await fetch(`https://huggingface.co/api/models?author=${HF_ORG}&sort=downloads&direction=-1&limit=50`);
      if (!resp.ok) throw new Error(`HF API: ${resp.status}`);

      const hfModels = await resp.json() as any[];
      const models: PublishedModelInfo[] = [];

      for (const m of hfModels) {
        const name = (m.id as string).split('/').pop() ?? '';
        if (name.includes('paper')) continue;

        models.push({
          id: m.id,
          name,
          baseModel: this.inferBaseModel(name),
          domain: this.inferDomain(name),
          improvementPct: 0,
          downloads: m.downloads ?? 0,
          likes: m.likes ?? 0,
          sizeGb: 0,
          variant: this.inferVariant(name),
          tags: m.tags ?? [],
          lastModified: m.lastModified ?? '',
        });
      }

      _cache = models;
      _cacheTime = Date.now();
      return models;
    } catch (err) {
      console.error('model/list-published: HF API error:', err);
      return _cache ?? [];
    }
  }

  private inferDomain(name: string): string {
    if (name.includes('-code-') || name.includes('coder-')) return 'code';
    if (name.includes('-reasoning-')) return 'reasoning';
    if (name.includes('-chat-')) return 'chat';
    return 'general';
  }

  private inferVariant(name: string): string {
    if (name.includes('-GGUF') || name.includes('-gguf')) return 'gguf';
    if (name.includes('mlx')) return 'mlx';
    if (name.includes('defrag')) return 'defragged';
    if (name.includes('compacted')) return 'compacted';
    return 'forged';
  }

  private inferBaseModel(name: string): string {
    let base = name.toLowerCase();
    for (const s of ['-gguf', '-mlx-4bit', '-defragged', '-compacted', '-forged']) {
      base = base.replace(s, '');
    }
    for (const d of ['-code', '-general', '-reasoning', '-chat']) {
      base = base.replace(new RegExp(`(?<=-)${d.slice(1)}(?=-|$)`), '');
    }
    base = base.replace(/--+/g, '-').replace(/^-|-$/g, '');

    if (base.startsWith('qwen')) {
      const parts = base.split('-').map(p => {
        if (p.startsWith('qwen')) return 'Qwen' + p.slice(4);
        if (p.match(/^\d+(\.\d+)?b$/)) return p.toUpperCase();
        if (p.match(/^a\d+b$/i)) return p.toUpperCase();
        return p.charAt(0).toUpperCase() + p.slice(1);
      });
      return `Qwen/${parts.join('-')}`;
    }
    return base;
  }
}
