/**
 * RustCoreIPC Plasticity Module - Neural plasticity optimization
 *
 * Wraps the Rust PlasticityModule IPC commands for analyzing utilization
 * data and compacting models based on per-head attention scoring.
 */

import type { RustCoreIPCClientBase } from './base';
import type {
	AnalysisResult,
	CompactionResult,
} from '../../../../shared/generated/plasticity';

// ============================================================================
// Types (camelCase for TypeScript consumers)
// ============================================================================

export interface PlasticityAnalyzeParams {
	adapterPath: string;
	config?: {
		minHeadsPerLayer?: number;
		minKvHeadsPerLayer?: number;
		deadThreshold?: number;
		lowThreshold?: number;
		highThreshold?: number;
		saturatedThreshold?: number;
		enableQuantization?: boolean;
	};
}

export interface PlasticityCompactParams {
	adapterPath: string;
	modelPath: string;
	outputPath?: string;
	config?: PlasticityAnalyzeParams['config'];
}

export interface PlasticityTopologyParams {
	topologyPath: string;
}

// ============================================================================
// Mixin
// ============================================================================

export interface PlasticityMixin {
	plasticityAnalyze(params: PlasticityAnalyzeParams): Promise<AnalysisResult>;
	plasticityCompact(params: PlasticityCompactParams): Promise<CompactionResult>;
	plasticityTopology(params: PlasticityTopologyParams): Promise<import('../../../../shared/generated/plasticity').HeadTopology>;
}

export function PlasticityMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements PlasticityMixin {
		/**
		 * Dry-run analysis: compute what compaction WOULD do without modifying files.
		 * Requires gate_gradients.json in the adapter directory.
		 */
		async plasticityAnalyze(params: PlasticityAnalyzeParams): Promise<AnalysisResult> {
			const response = await this.request({
				command: 'plasticity/analyze',
				adapterPath: params.adapterPath,
				...(params.config ? { config: params.config } : {}),
			});
			if (!response.success) throw new Error(response.error || 'plasticity/analyze failed');
			return response.result as AnalysisResult;
		}

		/**
		 * Compact a model: physically remove pruned heads and write compacted safetensors.
		 * Reads gate_gradients.json from adapter directory, slices base model weights.
		 */
		async plasticityCompact(params: PlasticityCompactParams): Promise<CompactionResult> {
			const response = await this.request({
				command: 'plasticity/compact',
				adapterPath: params.adapterPath,
				modelPath: params.modelPath,
				...(params.outputPath ? { outputPath: params.outputPath } : {}),
				...(params.config ? { config: params.config } : {}),
			});
			if (!response.success) throw new Error(response.error || 'plasticity/compact failed');
			return response.result as CompactionResult;
		}

		/**
		 * Get topology of an already-compacted model.
		 */
		async plasticityTopology(params: PlasticityTopologyParams): Promise<import('../../../../shared/generated/plasticity').HeadTopology> {
			const response = await this.request({
				command: 'plasticity/topology',
				topologyPath: params.topologyPath,
			});
			if (!response.success) throw new Error(response.error || 'plasticity/topology failed');
			return response.result as import('../../../../shared/generated/plasticity').HeadTopology;
		}
	};
}
