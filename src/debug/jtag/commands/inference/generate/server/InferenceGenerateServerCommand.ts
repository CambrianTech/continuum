/**
 * Inference Generate Command - Server Implementation
 *
 * Generate text using local or cloud AI inference. Auto-routes to best available backend
 * (Candle → cloud). Handles model loading, LoRA adapters, and provider failover automatically.
 *
 * This is the SINGLE ENTRY POINT for all text generation in the system.
 * PersonaUser, tools, and any other consumer should use this command.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { InferenceGenerateParams, InferenceGenerateResult } from '../shared/InferenceGenerateTypes';
import { createInferenceGenerateResultFromParams } from '../shared/InferenceGenerateTypes';
import { AIProviderDaemon } from '@daemons/ai-provider-daemon/shared/AIProviderDaemon';
import { LOCAL_MODELS } from '@system/shared/Constants';
import { AdapterStore } from '@system/genome/server/AdapterStore';

export class InferenceGenerateServerCommand extends CommandBase<InferenceGenerateParams, InferenceGenerateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('inference/generate', context, subpath, commander);
  }

  async execute(params: InferenceGenerateParams): Promise<InferenceGenerateResult> {
    const startTime = Date.now();

    // Validate required parameters
    if (!params.prompt || params.prompt.trim() === '') {
      throw new ValidationError(
        'prompt',
        `Missing required parameter 'prompt'. ` +
        `Use the help tool with 'inference/generate' or see the README for usage information.`
      );
    }

    // Resolve model - use LOCAL_MODELS.DEFAULT for local providers
    // Cloud providers should use their own defaults (handled by Rust adapter)
    const localProviders = ['candle', 'local', 'llamacpp'];
    const isLocalProvider = params.provider && localProviders.includes(params.provider.toLowerCase());
    const requestedModel = params.model || (isLocalProvider ? LOCAL_MODELS.DEFAULT : undefined);

    // Resolve and filter adapters to only those with existing files
    let adaptersToApply: Array<{ name: string; path: string; domain: string; scale: number }> = [];
    if (params.adapters && params.adapters.length > 0) {
      adaptersToApply = params.adapters
        .map(name => ({
          name,
          path: this._resolveAdapterPath(name),
          domain: 'general',
          scale: 1.0,
        }))
        .filter((adapter): adapter is { name: string; path: string; domain: string; scale: number } => {
          if (!adapter.path) {
            console.log(`🧬 inference/generate: Skipping adapter ${adapter.name} - not found in genome adapters or legacy paths`);
            return false;
          }
          console.log(`🧬 inference/generate: Resolved adapter ${adapter.name} → ${adapter.path}`);
          return true;
        });
    }

    // Build the generation request
    const generateRequest = {
      messages: [{ role: 'user' as const, content: params.prompt }],
      model: requestedModel,
      maxTokens: params.maxTokens || 2048,
      temperature: params.temperature || 0.7,
      systemPrompt: params.systemPrompt,
      activeAdapters: adaptersToApply.length > 0 ? adaptersToApply : undefined,
      provider: params.provider,
    };

    try {
      // Call AIProviderDaemon static method (auto-routes to best provider)
      const response = await AIProviderDaemon.generateText(generateRequest);

      const responseTimeMs = Date.now() - startTime;

      return createInferenceGenerateResultFromParams(params, {
        success: true,
        text: response.text,
        model: response.model,
        provider: response.provider,
        isLocal: response.routing?.isLocal ?? false,
        adaptersApplied: response.routing?.adaptersApplied ?? [],
        inputTokens: response.usage?.inputTokens ?? 0,
        outputTokens: response.usage?.outputTokens ?? 0,
        responseTimeMs,
      });
    } catch (error) {
      const responseTimeMs = Date.now() - startTime;
      const errorMessage = error instanceof Error ? error.message : String(error);

      // Log the error but return a structured failure
      console.error(`❌ inference/generate failed: ${errorMessage}`);

      return createInferenceGenerateResultFromParams(params, {
        success: false,
        text: '',
        model: requestedModel || params.model || 'unknown',
        provider: params.provider || 'unknown',
        isLocal: false,
        adaptersApplied: [],
        inputTokens: 0,
        outputTokens: 0,
        responseTimeMs,
        error: errorMessage,
      });
    }
  }

  /**
   * Resolve an adapter name to its filesystem path.
   * Delegates to AdapterStore — the SINGLE SOURCE OF TRUTH for adapter discovery.
   */
  private _resolveAdapterPath(name: string): string | null {
    // Search all adapters for a name match
    const all = AdapterStore.discoverAll();
    const match = all.find(a =>
      a.manifest.name === name ||
      a.dirPath.includes(name)
    );

    if (match && match.hasWeights) return match.dirPath;
    return null;
  }
}
