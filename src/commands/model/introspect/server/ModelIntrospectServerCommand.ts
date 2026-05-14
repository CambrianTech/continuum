/**
 * Model Introspect Command - Server Implementation
 *
 * Introspects a model to detect its architecture, capabilities, and which
 * ForgeRecipe stages can be applied. Returns the model's current state as
 * an alloy-compatible spec. Tries local HF cache first, then SSH to grid
 * nodes, then HF API.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { ModelIntrospectParams, ModelIntrospectResult } from '../shared/ModelIntrospectTypes';
import { createModelIntrospectResultFromParams } from '../shared/ModelIntrospectTypes';
import { execSync } from 'child_process';
import * as path from 'path';
import * as fs from 'fs';

/** Grid nodes discovered at runtime — no hardcoded IPs */
const SENTINEL_NODES: Array<{ name: string; ip: string }> = [];

export class ModelIntrospectServerCommand extends CommandBase<ModelIntrospectParams, ModelIntrospectResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('model/introspect', context, subpath, commander);
  }

  async execute(params: ModelIntrospectParams): Promise<ModelIntrospectResult> {
    if (!params.model || params.model.trim() === '') {
      throw new ValidationError('model', 'Missing required parameter \'model\'. Example: Qwen/Qwen3.5-4B');
    }

    const model = params.model.trim();

    // Try local sentinel-ai first
    let result = this.tryLocalIntrospect(model);

    // Try remote nodes
    if (!result) {
      for (const node of SENTINEL_NODES) {
        result = this.tryRemoteIntrospect(model, node.ip);
        if (result) break;
      }
    }

    if (!result) {
      // Minimal introspection from HF config without sentinel-ai
      result = this.tryMinimalIntrospect(model);
    }

    if (!result) {
      return createModelIntrospectResultFromParams(params, {
        success: false,
        source: { baseModel: model, architecture: 'unknown', isMoE: false },
        currentCapabilities: {},
        possibleStages: [],
        currentAlloy: { name: model, version: '1.0.0', source: { baseModel: model }, stages: [], cycles: 1 },
      } as any);
    }

    return createModelIntrospectResultFromParams(params, {
      success: true,
      source: result.source,
      currentCapabilities: result.currentCapabilities,
      possibleStages: result.possibleStages,
      currentAlloy: result.currentAlloy,
    });
  }

  private tryLocalIntrospect(model: string): any {
    const sentinelPaths = [
      path.join(process.env.HOME ?? '', 'sentinel-ai'),
      path.join(process.cwd(), '..', 'sentinel-ai'),
    ];

    for (const sentinelPath of sentinelPaths) {
      const script = path.join(sentinelPath, 'scripts', 'stages', 'introspect.py');
      if (!fs.existsSync(script)) continue;

      try {
        const output = execSync(
          `cd ${sentinelPath} && python3 scripts/stages/introspect.py "${model}"`,
          { timeout: 15000, encoding: 'utf-8' }
        );
        return JSON.parse(output.trim());
      } catch {
        continue;
      }
    }
    return null;
  }

  private tryRemoteIntrospect(model: string, ip: string): any {
    const home = process.env.HOME ?? '';
    try {
      const output = execSync(
        `ssh -i ${home}/.ssh/id_ed25519 -o ConnectTimeout=3 -o StrictHostKeyChecking=no joel@${ip} "cd ~/sentinel-ai && python3 scripts/stages/introspect.py '${model}'" 2>/dev/null`,
        { timeout: 15000, encoding: 'utf-8' }
      );
      return JSON.parse(output.trim());
    } catch {
      return null;
    }
  }

  private tryMinimalIntrospect(model: string): any {
    // Fallback: try to read config.json from HF cache
    const home = process.env.HOME ?? '';
    const slug = `models--${model.replace('/', '--')}`;
    const cacheDir = path.join(home, '.cache', 'huggingface', 'hub', slug, 'snapshots');

    if (!fs.existsSync(cacheDir)) return null;

    try {
      const snapshots = fs.readdirSync(cacheDir);
      for (const snap of snapshots.sort().reverse()) {
        const configPath = path.join(cacheDir, snap, 'config.json');
        if (fs.existsSync(configPath)) {
          const config = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
          const textConfig = config.text_config || config;
          const arch = (config.model_type || 'unknown').includes('qwen') ? 'qwen3_5' : config.model_type || 'unknown';

          return {
            source: { baseModel: model, architecture: arch, isMoE: !!config.num_experts },
            currentCapabilities: {
              architecture: arch,
              layers: textConfig.num_hidden_layers,
              heads: textConfig.num_attention_heads,
              hiddenSize: textConfig.hidden_size,
              contextLength: textConfig.max_position_embeddings,
              modalities: config.vision_config ? ['text', 'vision'] : ['text'],
            },
            possibleStages: [],
            currentAlloy: {
              name: model.split('/').pop()?.toLowerCase(),
              version: '1.0.0',
              source: { baseModel: model, architecture: arch },
              stages: [],
              cycles: 1,
            },
          };
        }
      }
    } catch { /* */ }
    return null;
  }
}
