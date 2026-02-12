/**
 * RustCoreIPC Memory Module - Hippocampus memory subsystem methods
 */

import type { RustCoreIPCClientBase } from './base';
import type { CorpusMemory } from '../CorpusMemory';
import type { CorpusTimelineEvent } from '../CorpusTimelineEvent';
import type { LoadCorpusResponse } from '../LoadCorpusResponse';
import type { MemoryRecallResponse } from '../MemoryRecallResponse';
import type { MultiLayerRecallRequest } from '../MultiLayerRecallRequest';
import type { ConsciousnessContextResponse } from '../ConsciousnessContextResponse';

// ============================================================================
// Mixin
// ============================================================================

export interface MemoryMixin {
	memoryLoadCorpus(personaId: string, memories: CorpusMemory[], events: CorpusTimelineEvent[]): Promise<LoadCorpusResponse>;
	memoryAppendMemory(personaId: string, memory: CorpusMemory): Promise<void>;
	memoryAppendEvent(personaId: string, event: CorpusTimelineEvent): Promise<void>;
	memoryMultiLayerRecall(personaId: string, params: MultiLayerRecallRequest): Promise<MemoryRecallResponse>;
	memoryConsciousnessContext(personaId: string, roomId: string, currentMessage?: string, skipSemanticSearch?: boolean): Promise<ConsciousnessContextResponse>;
}

export function MemoryMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements MemoryMixin {
		/**
		 * Load memory corpus into Rust cache.
		 * Called at persona startup — sends all memories + timeline events from TS ORM.
		 * Subsequent recall operations run on this cached corpus.
		 */
		async memoryLoadCorpus(
			personaId: string,
			memories: CorpusMemory[],
			events: CorpusTimelineEvent[]
		): Promise<LoadCorpusResponse> {
			const response = await this.request({
				command: 'memory/load-corpus',
				persona_id: personaId,
				memories,
				events,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to load memory corpus');
			}

			return response.result as LoadCorpusResponse;
		}

		/**
		 * Append a single memory to the cached corpus (incremental update).
		 * Called after Hippocampus stores a new memory to the DB.
		 * Keeps Rust cache coherent with the ORM without full reload.
		 */
		async memoryAppendMemory(
			personaId: string,
			memory: CorpusMemory
		): Promise<void> {
			const response = await this.request({
				command: 'memory/append-memory',
				persona_id: personaId,
				memory,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to append memory to corpus');
			}
		}

		/**
		 * Append a single timeline event to the cached corpus (incremental update).
		 */
		async memoryAppendEvent(
			personaId: string,
			event: CorpusTimelineEvent
		): Promise<void> {
			const response = await this.request({
				command: 'memory/append-event',
				persona_id: personaId,
				event,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to append event to corpus');
			}
		}

		/**
		 * 6-layer parallel multi-recall (the big improvement)
		 */
		async memoryMultiLayerRecall(
			personaId: string,
			params: MultiLayerRecallRequest
		): Promise<MemoryRecallResponse> {
			const response = await this.request({
				command: 'memory/multi-layer-recall',
				persona_id: personaId,
				...params,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to run multi-layer recall');
			}

			return response.result as MemoryRecallResponse;
		}

		/**
		 * Build consciousness context for RAG injection
		 * Replaces UnifiedConsciousness.getContext() in TS
		 */
		async memoryConsciousnessContext(
			personaId: string,
			roomId: string,
			currentMessage?: string,
			skipSemanticSearch?: boolean
		): Promise<ConsciousnessContextResponse> {
			const response = await this.request({
				command: 'memory/consciousness-context',
				persona_id: personaId,
				room_id: roomId,
				current_message: currentMessage ?? null,
				skip_semantic_search: skipSemanticSearch ?? false,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to build consciousness context');
			}

			return response.result as ConsciousnessContextResponse;
		}
	};
}
