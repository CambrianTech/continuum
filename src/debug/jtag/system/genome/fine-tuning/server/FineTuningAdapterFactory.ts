/**
 * FineTuningAdapterFactory - Returns the right fine-tuning adapter for each provider
 *
 * Each provider has its own training backend:
 * - Ollama: Local LoRA via llama.cpp
 * - OpenAI: Fine-tuning API (gpt-4o-mini, gpt-3.5-turbo)
 * - Together: Fine-tuning API
 * - Fireworks: Fine-tuning API
 * - DeepSeek: Fine-tuning API
 * - Mistral: Fine-tuning API
 * - Anthropic: Enterprise only (no public API)
 * - Groq: No fine-tuning (inference only)
 * - xAI: No fine-tuning (inference only)
 */

import { BaseLoRATrainerServer } from './BaseLoRATrainerServer';
import { OllamaLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/ollama/server/OllamaFineTuningAdapter';
import { OpenAILoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/openai/server/OpenAIFineTuningAdapter';
import { TogetherLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/together/server/TogetherFineTuningAdapter';
import { FireworksLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/fireworks/server/FireworksFineTuningAdapter';
import { DeepSeekLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/deepseek/server/DeepSeekFineTuningAdapter';
import { MistralLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/mistral/server/MistralFineTuningAdapter';

/**
 * Provider IDs that support fine-tuning
 */
export type FineTuningProvider =
  | 'ollama'
  | 'openai'
  | 'together'
  | 'fireworks'
  | 'deepseek'
  | 'mistral';

/**
 * Provider IDs that don't support fine-tuning
 */
export type NoFineTuningProvider =
  | 'anthropic'  // Enterprise only
  | 'groq'       // Inference only
  | 'xai';       // Inference only

/**
 * All provider IDs
 */
export type ProviderType = FineTuningProvider | NoFineTuningProvider | string;

/**
 * Adapter cache (singleton per provider)
 */
const adapterCache: Map<string, BaseLoRATrainerServer> = new Map();

/**
 * Get fine-tuning adapter for a provider
 *
 * @param provider - Provider ID (ollama, openai, together, etc.)
 * @returns Fine-tuning adapter or null if provider doesn't support fine-tuning
 */
export function getFineTuningAdapter(provider: ProviderType): BaseLoRATrainerServer | null {
  // Check cache first
  if (adapterCache.has(provider)) {
    return adapterCache.get(provider)!;
  }

  let adapter: BaseLoRATrainerServer | null = null;

  switch (provider.toLowerCase()) {
    case 'ollama':
      adapter = new OllamaLoRAAdapter();
      break;

    case 'openai':
      adapter = new OpenAILoRAAdapter();
      break;

    case 'together':
      adapter = new TogetherLoRAAdapter();
      break;

    case 'fireworks':
      adapter = new FireworksLoRAAdapter();
      break;

    case 'deepseek':
      adapter = new DeepSeekLoRAAdapter();
      break;

    case 'mistral':
      adapter = new MistralLoRAAdapter();
      break;

    // Providers without fine-tuning support
    case 'anthropic':
    case 'groq':
    case 'xai':
      return null;

    default:
      // Unknown provider - try Ollama as fallback for local models
      console.warn(`[FineTuningFactory] Unknown provider "${provider}", falling back to Ollama`);
      adapter = new OllamaLoRAAdapter();
      break;
  }

  // Cache the adapter
  if (adapter) {
    adapterCache.set(provider, adapter);
  }

  return adapter;
}

/**
 * Check if a provider supports fine-tuning
 */
export function supportsFineTuning(provider: ProviderType): boolean {
  const adapter = getFineTuningAdapter(provider);
  return adapter?.supportsFineTuning() ?? false;
}

/**
 * Get list of providers that support fine-tuning
 */
export function getFineTuningProviders(): FineTuningProvider[] {
  return ['ollama', 'openai', 'together', 'fireworks', 'deepseek', 'mistral'];
}

/**
 * Clear adapter cache (for testing)
 */
export function clearAdapterCache(): void {
  adapterCache.clear();
}
