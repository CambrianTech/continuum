/**
 * RustCoreIPC AI Module - Text generation via Rust AI providers
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
// ============================================================================

export interface AIGenerateParams {
	messages?: Array<{ role: string; content: string }>;
	prompt?: string;
	provider?: string;
	model?: string;
	maxTokens?: number;
	temperature?: number;
	systemPrompt?: string;
	requestId?: string;
	userId?: string;
	roomId?: string;
	purpose?: string;
}

export interface AIGenerateResult {
	text: string;
	model: string;
	provider: string;
	finishReason: string;
	usage: {
		inputTokens: number;
		outputTokens: number;
		totalTokens: number;
		estimatedCost?: number;
	};
	responseTimeMs: number;
	requestId: string;
	routing?: {
		provider: string;
		isLocal: boolean;
		routingReason: string;
		adaptersApplied: string[];
	};
	toolCalls?: Array<{
		id: string;
		type: string;
		function: {
			name: string;
			arguments: string;
		};
	}>;
}

// ============================================================================
// Mixin
// ============================================================================

export interface AIMixin {
	aiGenerate(params: AIGenerateParams): Promise<AIGenerateResult>;
}

export function AIMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements AIMixin {
		/**
		 * Generate text using Rust AI provider module
		 */
		async aiGenerate(params: AIGenerateParams): Promise<AIGenerateResult> {
			const response = await this.request({
				command: 'ai/generate',
				...params,
			});

			if (!response.success) {
				throw new Error(response.error || 'AI generation failed');
			}

			const result = response.result;
			return {
				text: result.text || '',
				model: result.model || 'unknown',
				provider: result.provider || 'unknown',
				finishReason: result.finishReason || 'stop',
				usage: {
					inputTokens: result.usage?.inputTokens || 0,
					outputTokens: result.usage?.outputTokens || 0,
					totalTokens: result.usage?.totalTokens || 0,
					estimatedCost: result.usage?.estimatedCost,
				},
				responseTimeMs: result.responseTimeMs || 0,
				requestId: result.requestId || '',
				routing: result.routing ? {
					provider: result.routing.provider,
					isLocal: result.routing.isLocal,
					routingReason: result.routing.routingReason,
					adaptersApplied: result.routing.adaptersApplied || [],
				} : undefined,
				toolCalls: result.toolCalls,
			};
		}
	};
}
