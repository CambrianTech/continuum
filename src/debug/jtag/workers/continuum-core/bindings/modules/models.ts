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

// ============================================================================
// Mixin
// ============================================================================

export interface ModelsMixin {
	modelsDiscover(providers: ProviderConfig[]): Promise<ModelsDiscoverResult>;
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
	};
}
