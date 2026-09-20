/**
 * Adapter Try Command - Server Implementation
 *
 * Temporarily load a LoRA adapter and run A/B comparison test.
 * Allows personas to evaluate adapters before adopting them.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { AdapterTryParams, AdapterTryResult } from '../shared/AdapterTryTypes';
import { createAdapterTryResultFromParams } from '../shared/AdapterTryTypes';
import { InferenceGrpcClient } from '@system/core/services/InferenceGrpcClient';

export class AdapterTryServerCommand extends CommandBase<AdapterTryParams, AdapterTryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('adapter/try', context, subpath, commander);
  }

  async execute(params: AdapterTryParams): Promise<AdapterTryResult> {
    console.log('🧪 Adapter Try: Testing', params.adapterId);

    // Validate required parameters
    if (!params.adapterId || params.adapterId.trim() === '') {
      throw new ValidationError(
        'adapterId',
        `Missing required parameter 'adapterId'. ` +
        `Use adapter/search to find adapters, then provide the ID here.`
      );
    }

    if (!params.testPrompt || params.testPrompt.trim() === '') {
      throw new ValidationError(
        'testPrompt',
        `Missing required parameter 'testPrompt'. ` +
        `Provide a prompt to test the adapter's behavior.`
      );
    }

    const scale = params.scale ?? 1.0;
    const maxTokens = params.maxTokens ?? 100;

    const client = new InferenceGrpcClient();

    try {
      // Step 1: Check connection (ping throws if worker unavailable)
      await client.ping();

      // Step 2: Get current model
      const status = await client.status();
      const currentModel = status.currentModel;

      // Step 3: Generate BASELINE (without adapter)
      console.log('   Generating baseline...');
      const baselineStart = Date.now();
      const baselineResult = await client.generate(currentModel, params.testPrompt, {
        maxTokens,
        temperature: 0.7,
        timeoutMs: 60000,
      });
      const baselineTimeMs = Date.now() - baselineStart;

      // Step 4: Download/load adapter if not already installed
      console.log(`   Loading adapter: ${params.adapterId}...`);
      const adapterId = params.adapterId.includes('/')
        ? params.adapterId.replace('/', '--')
        : params.adapterId;

      // Check if already loaded
      const existingAdapters = await client.listAdapters();
      const alreadyLoaded = existingAdapters.some(a =>
        a.adapterId === adapterId || a.adapterId === params.adapterId
      );

      let adapterMetadata: Record<string, unknown> = {};

      if (!alreadyLoaded) {
        // Download from HuggingFace if needed
        if (params.adapterId.includes('/')) {
          console.log('   Downloading from HuggingFace...');
          const download = await client.downloadAdapter(params.adapterId, {
            adapterId,
            scale,
          });
          if (!download.success) {
            throw new Error(`Failed to download adapter: ${download.error}`);
          }
          if (download.metadata) {
            adapterMetadata = {
              baseModel: download.metadata.baseModel,
              rank: download.metadata.rank,
              alpha: download.metadata.alpha,
              targetModules: download.metadata.targetModules,
            };
          }
        }
      }

      // Step 5: Apply adapter via genome
      console.log('   Applying adapter...');
      const genome = await client.applyGenome([
        { adapterId, scale }
      ]);
      console.log(`   Applied ${genome.adaptersApplied} adapter, ${genome.layersMerged} layers`);

      // Step 6: Generate WITH ADAPTER
      console.log('   Generating with adapter...');
      const adapterStart = Date.now();
      const adapterResult = await client.generate(currentModel, params.testPrompt, {
        maxTokens,
        temperature: 0.7,
        timeoutMs: 60000,
      });
      const adapterTimeMs = Date.now() - adapterStart;

      // Step 7: Unload adapter (restore baseline state)
      console.log('   Unloading adapter...');
      await client.unloadAdapter(adapterId);

      // Restore base model (no adapters)
      await client.applyGenome([]);

      console.log('   ✅ A/B test complete');

      return createAdapterTryResultFromParams(params, {
        success: true,
        adapterId: params.adapterId,
        baselineOutput: baselineResult.text.trim(),
        adapterOutput: adapterResult.text.trim(),
        baselineTimeMs,
        adapterTimeMs,
        adapterMetadata,
      });

    } catch (error) {
      console.error('   ❌ Adapter try failed:', error);
      return createAdapterTryResultFromParams(params, {
        success: false,
        adapterId: params.adapterId,
        baselineOutput: '',
        adapterOutput: '',
        baselineTimeMs: 0,
        adapterTimeMs: 0,
        adapterMetadata: {},
        error: {
          type: 'adapter_try_failed',
          message: String(error),
        },
      });
    } finally {
      client.close();
    }
  }
}
