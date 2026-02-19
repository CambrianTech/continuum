/**
 * RustCoreIPC Embedding Module - Vector embedding operations
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types
// ============================================================================

export interface EmbeddingResult {
	embedding: number[];
	model: string;
	dimensions: number;
}

export interface SimilarityResult {
	score: number;
}

export interface TopKResult {
	index: number;
	similarity: number;
}

export interface ClusterResult {
	clusterId: number;
	members: string[];
	centroid?: number[];
}

// ============================================================================
// Mixin
// ============================================================================

export interface TopKResponse {
	results: TopKResult[];
	count: number;
	totalTargets: number;
	durationMs: number;
}

export interface EmbeddingMixin {
	embeddingGenerate(text: string, model?: string): Promise<EmbeddingResult>;
	embeddingGenerateBatch(texts: string[], model?: string): Promise<EmbeddingResult[]>;
	embeddingSimilarity(a: number[], b: number[]): Promise<SimilarityResult>;
	embeddingSimilarityMatrix(embeddings: number[][]): Promise<{ matrix: Float32Array; n: number }>;
	embeddingTopK(query: number[], targets: number[][], k?: number, threshold?: number): Promise<TopKResponse>;
	embeddingCluster(embeddings: Array<{ id: string; embedding: number[] }>, numClusters: number): Promise<ClusterResult[]>;
}

export function EmbeddingMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements EmbeddingMixin {
		/**
		 * Generate embedding for a single text
		 */
		async embeddingGenerate(text: string, model?: string): Promise<EmbeddingResult> {
			const response = await this.request({
				command: 'embedding/generate',
				text,
				model: model ?? 'default',
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to generate embedding');
			}

			return response.result as EmbeddingResult;
		}

		/**
		 * Generate embeddings for multiple texts in batch
		 */
		async embeddingGenerateBatch(texts: string[], model?: string): Promise<EmbeddingResult[]> {
			const response = await this.request({
				command: 'embedding/batch',
				texts,
				model: model ?? 'default',
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to generate batch embeddings');
			}

			return response.result?.embeddings ?? [];
		}

		/**
		 * Calculate cosine similarity between two embeddings
		 */
		async embeddingSimilarity(a: number[], b: number[]): Promise<SimilarityResult> {
			const response = await this.request({
				command: 'embedding/similarity',
				a,
				b,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to calculate similarity');
			}

			return { score: response.result?.score ?? 0 };
		}

		/**
		 * Calculate pairwise similarity matrix (returns binary Float32Array)
		 */
		async embeddingSimilarityMatrix(embeddings: number[][]): Promise<{ matrix: Float32Array; n: number }> {
			const { response, binaryData } = await this.requestFull({
				command: 'embedding/similarity-matrix',
				embeddings,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to calculate similarity matrix');
			}

			if (!binaryData) {
				throw new Error('No binary data in similarity matrix response');
			}

			const n = embeddings.length;
			const matrix = new Float32Array(binaryData.buffer, binaryData.byteOffset, binaryData.byteLength / 4);

			return { matrix, n };
		}

		/**
		 * Find top-K most similar embeddings to query
		 */
		async embeddingTopK(
			query: number[],
			targets: number[][],
			k: number = 10,
			threshold: number = 0.0
		): Promise<TopKResponse> {
			const response = await this.request({
				command: 'embedding/top-k',
				query,
				targets,
				k,
				threshold,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to find top-k similar');
			}

			return {
				results: response.result?.results || [],
				count: response.result?.count as number,
				totalTargets: response.result?.totalTargets as number,
				durationMs: response.result?.durationMs as number,
			};
		}

		/**
		 * Cluster embeddings using k-means
		 */
		async embeddingCluster(
			embeddings: Array<{ id: string; embedding: number[] }>,
			numClusters: number
		): Promise<ClusterResult[]> {
			const response = await this.request({
				command: 'embedding/cluster',
				embeddings,
				num_clusters: numClusters,
			});

			if (!response.success) {
				throw new Error(response.error || 'Failed to cluster embeddings');
			}

			return response.result?.clusters ?? [];
		}
	};
}
