/**
 * RustCoreIPC Search Module - Algorithm-based search methods
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
// ============================================================================

export interface SearchExecuteResult {
	algorithm: string;
	scores: number[];
	rankedIndices: number[];
}

export interface SearchVectorResult {
	scores: number[];
	rankedIndices: number[];
}

// ============================================================================
// Mixin
// ============================================================================

export interface SearchMixin {
	searchList(): Promise<string[]>;
	searchExecute(query: string, corpus: string[], algorithm?: string, params?: Record<string, unknown>): Promise<SearchExecuteResult>;
	searchVector(queryVector: number[], corpusVectors: number[][], normalize?: boolean, threshold?: number): Promise<SearchVectorResult>;
	searchParams(algorithm: string): Promise<{ params: string[]; values: Record<string, unknown> }>;
}

export function SearchMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements SearchMixin {
		/**
		 * List available search algorithms
		 */
		async searchList(): Promise<string[]> {
			const response = await this.request({ command: 'search/list' });
			if (!response.success) throw new Error(response.error || 'Failed to list search algorithms');
			return response.result?.algorithms || [];
		}

		/**
		 * Execute text search using specified algorithm
		 */
		async searchExecute(
			query: string,
			corpus: string[],
			algorithm: string = 'bm25',
			params?: Record<string, unknown>
		): Promise<SearchExecuteResult> {
			const response = await this.request({
				command: 'search/execute',
				algorithm,
				query,
				corpus,
				params: params ?? null,
			});
			if (!response.success) throw new Error(response.error || 'Search execution failed');
			return {
				algorithm: response.result?.algorithm || algorithm,
				scores: response.result?.scores || [],
				rankedIndices: response.result?.rankedIndices || [],
			};
		}

		/**
		 * Vector similarity search using cosine similarity
		 */
		async searchVector(
			queryVector: number[],
			corpusVectors: number[][],
			normalize: boolean = true,
			threshold: number = 0.0
		): Promise<SearchVectorResult> {
			const response = await this.request({
				command: 'search/vector',
				queryVector,
				corpusVectors,
				normalize,
				threshold,
			});
			if (!response.success) throw new Error(response.error || 'Vector search failed');
			return {
				scores: response.result?.scores || [],
				rankedIndices: response.result?.rankedIndices || [],
			};
		}

		/**
		 * Get algorithm parameters and current values
		 */
		async searchParams(algorithm: string): Promise<{ params: string[]; values: Record<string, unknown> }> {
			const response = await this.request({ command: 'search/params', algorithm });
			if (!response.success) throw new Error(response.error || 'Failed to get search params');
			return {
				params: response.result?.params || [],
				values: response.result?.values || {},
			};
		}
	};
}
