/**
 * RustCoreIPC RAG Module - Retrieval-Augmented Generation context building
 */

import type { RustCoreIPCClientBase } from './base';
import type { RagSourceRequest, RagComposeResult } from '../../../../shared/generated/rag';

// ============================================================================
// Mixin
// ============================================================================

export interface RagMixin {
	ragCompose(
		personaId: string,
		roomId: string,
		sources: RagSourceRequest[],
		queryText?: string,
		totalBudget?: number
	): Promise<RagComposeResult>;
}

export function RagMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements RagMixin {
		/**
		 * Compose RAG context from multiple sources in parallel (Rayon).
		 * Batches source loading for maximum throughput.
		 *
		 * @param personaId - Persona requesting context
		 * @param roomId - Room context for conversation
		 * @param sources - Array of source requests to load
		 * @param queryText - Optional query for semantic search
		 * @param totalBudget - Total token budget across all sources
		 */
		async ragCompose(
			personaId: string,
			roomId: string,
			sources: RagSourceRequest[],
			queryText?: string,
			totalBudget: number = 2000
		): Promise<RagComposeResult> {
			const response = await this.request({
				command: 'rag/compose',
				persona_id: personaId,
				room_id: roomId,
				sources,
				query_text: queryText ?? null,
				total_budget: totalBudget,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to compose RAG context');
			}

			return response.result as RagComposeResult;
		}
	};
}
