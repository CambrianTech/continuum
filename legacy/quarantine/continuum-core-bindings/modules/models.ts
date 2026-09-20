/**
 * RustCoreIPC Models Module - AI model discovery and metadata
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
// ============================================================================

export interface ProviderConfig {
	provider_id: string;
	api_key: string;
	base_url: string;
	static_models?: Array<{
		id: string;
		context_window: number;
		max_output_tokens?: number;
		capabilities?: string[];
		cost_per_1k_tokens?: { input: number; output: number };
	}>;
}

export interface DiscoveredModel {
	modelId: string;
	contextWindow: number;
	maxOutputTokens?: number;
	provider: string;
	capabilities?: string[];
	costPer1kTokens?: { input: number; output: number };
	discoveredAt: number;
}

export interface ModelsDiscoverResult {
	models: DiscoveredModel[];
	count: number;
	providers: number;
}

/**
 * Result of `models/capabilities` — the canonical kebab-case capability
 * vocabulary for a model, as declared in `models.toml`. Strings match
 * Rust `model_registry::types::Capability` serde rename: "vision",
 * "audio-input", "audio-output", "tool-use", "streaming", etc.
 */
export interface ModelsCapabilitiesResult {
	modelId: string;
	capabilities: string[];
}

// ============================================================================
// Mixin
// ============================================================================

export interface ModelsMixin {
	modelsDiscover(providers: ProviderConfig[]): Promise<ModelsDiscoverResult>;
	modelsCapabilities(modelId: string): Promise<ModelsCapabilitiesResult>;
}

export function ModelsMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements ModelsMixin {
		/**
		 * Discover available models from specified providers
		 */
		async modelsDiscover(providers: ProviderConfig[]): Promise<ModelsDiscoverResult> {
			const response = await this.request({
				command: 'models/discover',
				providers,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to discover models');
			}

			return response.result as ModelsDiscoverResult;
		}

		/**
		 * Look up a model's canonical capability vocabulary from models.toml.
		 *
		 * Callers (PersonaResponseGenerator) use this ONCE at persona
		 * construction to resolve the capability strings they must then
		 * pass with every `cognitionPersonaRespond` call. Pushing this
		 * lookup to the orchestration seam (caller side, loud failure)
		 * means the inference hot path never does a global registry
		 * query whose silent-empty result used to disable vision.
		 *
		 * Errors visibly if the model id isn't in the registry — that's
		 * a broken persona configuration, not a missing-default
		 * scenario. No silent empty-list fallback.
		 */
		async modelsCapabilities(modelId: string): Promise<ModelsCapabilitiesResult> {
			const response = await this.request({
				command: 'models/capabilities',
				model_id: modelId,
			});

			if (!response.success) {
				throw new Error(response.error || `Failed to resolve capabilities for model '${modelId}'`);
			}

			return response.result as ModelsCapabilitiesResult;
		}
	};
}
