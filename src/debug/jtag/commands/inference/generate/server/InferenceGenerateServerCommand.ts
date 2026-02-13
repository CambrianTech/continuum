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
import { existsSync } from 'fs';
import { resolve } from 'path';

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

    // Filter adapters to only those with existing files
    let adaptersToApply: Array<{ name: string; path: string; domain: string }> = [];
    if (params.adapters && params.adapters.length > 0) {
      // Convert adapter names to full adapter specs
      // For now, use convention: adapter name -> ./lora-adapters/{name}.safetensors
      adaptersToApply = params.adapters
        .map(name => ({
          name,
          path: `./lora-adapters/${name}.safetensors`,
          domain: 'general'
        }))
        .filter(adapter => {
          const absolutePath = resolve(adapter.path);
          if (!existsSync(absolutePath)) {
            console.log(`🧬 inference/generate: Skipping adapter ${adapter.name} - file not found at ${absolutePath}`);
            return false;
          }
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
      preferredProvider: params.provider,
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
}
