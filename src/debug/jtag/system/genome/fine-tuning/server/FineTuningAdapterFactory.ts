/**
 * FineTuningAdapterFactory - Returns the right fine-tuning adapter for each provider
 *
 * Each provider has its own training backend:
 * - Local (PEFT): Native LoRA training with HuggingFace PEFT library
 * - OpenAI: Fine-tuning API (gpt-4o-mini, gpt-3.5-turbo)
 * - Together: Fine-tuning API
 * - Fireworks: Fine-tuning API
 * - DeepSeek: Fine-tuning API
 * - Mistral: Fine-tuning API
 * - Anthropic: Enterprise only (no public API)
 * - Groq: No fine-tuning (inference only)
 * - xAI: No fine-tuning (inference only)
 *
 * NOTE: All local training uses PEFT (native Python/HuggingFace).
 */

import { BaseLoRATrainer } from '../shared/BaseLoRATrainer';
import { OpenAILoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/openai/server/OpenAIFineTuningAdapter';
import { TogetherLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/together/server/TogetherFineTuningAdapter';
import { FireworksLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/fireworks/server/FireworksFineTuningAdapter';
import { DeepSeekLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/deepseek/server/DeepSeekFineTuningAdapter';
import { MistralLoRAAdapter } from '../../../../daemons/ai-provider-daemon/adapters/mistral/server/MistralFineTuningAdapter';
import { PEFTLoRAAdapter } from './adapters/PEFTLoRAAdapter';

/**
 * Provider IDs that support fine-tuning
 */
export type FineTuningProvider =
  | 'candle'   // Native Rust inference (uses PEFT for training)
  | 'local'    // Alias for candle
  | 'openai'
  | 'together'
  | 'fireworks'
  | 'deepseek'
  | 'mistral'
  | 'peft';

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
const adapterCache: Map<string, BaseLoRATrainer> = new Map();

/**
 * Get fine-tuning adapter for a provider
 *
 * @param provider - Provider ID (candle, peft, openai, together, etc.)
 * @returns Fine-tuning adapter or null if provider doesn't support fine-tuning
 */
export function getFineTuningAdapter(provider: ProviderType): BaseLoRATrainer | null {
  // Check cache first
  if (adapterCache.has(provider)) {
    return adapterCache.get(provider)!;
  }

  let adapter: BaseLoRATrainer | null = null;

  switch (provider.toLowerCase()) {
    // Local training - all use PEFT (native HuggingFace)
    case 'candle':
    case 'local':
    case 'peft':
      adapter = new PEFTLoRAAdapter();
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
      // Unknown provider - use PEFT for local training (works with any model)
      console.warn(`[FineTuningFactory] Unknown provider "${provider}", falling back to PEFT`);
      adapter = new PEFTLoRAAdapter();
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
  return ['peft', 'candle', 'local', 'openai', 'together', 'fireworks', 'deepseek', 'mistral'];
}

/**
 * Clear adapter cache (for testing)
 */
export function clearAdapterCache(): void {
  adapterCache.clear();
}
