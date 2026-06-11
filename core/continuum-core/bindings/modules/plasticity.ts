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
	CompressionPipelineResult,
	HeadTopology,
} from '../../../../protocol/typescript/plasticity';

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

export interface PlasticityCompressParams {
	capturePath: string;
	modelPath: string;
	deviceSpec?: string;
	outputPath?: string;
	architecture?: string;
}

export interface PlasticityPipelineParams {
	capturePath: string;
	modelPath: string;
	outputPath?: string;
	config?: PlasticityAnalyzeParams['config'];
}

// ============================================================================
// Mixin
// ============================================================================

export interface PlasticityMixin {
	plasticityAnalyze(params: PlasticityAnalyzeParams): Promise<AnalysisResult>;
	plasticityCompact(params: PlasticityCompactParams): Promise<CompactionResult>;
	plasticityTopology(params: PlasticityTopologyParams): Promise<HeadTopology>;
	plasticityCompress(params: PlasticityCompressParams): Promise<CompressionPipelineResult>;
	plasticityPipeline(params: PlasticityPipelineParams): Promise<CompactionResult>;
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
		async plasticityTopology(params: PlasticityTopologyParams): Promise<HeadTopology> {
			const response = await this.request({
				command: 'plasticity/topology',
				topologyPath: params.topologyPath,
			});
			if (!response.success) throw new Error(response.error || 'plasticity/topology failed');
			return response.result as HeadTopology;
		}

		/**
		 * Compress a model to mixed-quantization GGUF, fitted to a target device.
		 * Takes gate gradient capture + base model → produces optimized GGUF.
		 */
		async plasticityCompress(params: PlasticityCompressParams): Promise<CompressionPipelineResult> {
			const response = await this.request({
				command: 'plasticity/compress',
				capturePath: params.capturePath,
				modelPath: params.modelPath,
				...(params.deviceSpec ? { deviceSpec: params.deviceSpec } : {}),
				...(params.outputPath ? { outputPath: params.outputPath } : {}),
				...(params.architecture ? { architecture: params.architecture } : {}),
			});
			if (!response.success) throw new Error(response.error || 'plasticity/compress failed');
			return response.result as CompressionPipelineResult;
		}

		/**
		 * End-to-end pipeline: gate_gradients.json → analysis → compaction.
		 * The "wake up to a compacted model" command.
		 */
		async plasticityPipeline(params: PlasticityPipelineParams): Promise<CompactionResult> {
			const response = await this.request({
				command: 'plasticity/pipeline',
				capturePath: params.capturePath,
				modelPath: params.modelPath,
				...(params.outputPath ? { outputPath: params.outputPath } : {}),
				...(params.config ? { config: params.config } : {}),
			});
			if (!response.success) throw new Error(response.error || 'plasticity/pipeline failed');
			return response.result as CompactionResult;
		}
	};
}
