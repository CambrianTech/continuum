/**
 * RustCoreIPC Dataset Module — Training dataset import and management
 *
 * Thin TypeScript wrapper over the Rust DatasetModule.
 * All heavy I/O (CSV parsing, JSONL conversion, file writes) happens in Rust.
 */

import type { RustCoreIPCClientBase } from './base';

// ============================================================================
// Types (camelCase for TypeScript consumers)
// ============================================================================

export interface DatasetManifest {
	name: string;
	version: string;
	source?: string;
	totalExamples: number;
	trainExamples: number;
	evalExamples: number;
	trainPath: string;
	evalPath: string;
	metrics?: DatasetMetrics;
	preCutoff?: number;
	postCutoff?: number;
	importedAt: string;
}

export interface DatasetMetrics {
	avgCyclomaticComplexity?: number;
	avgLinesOfCode?: number;
}

export interface DatasetListResult {
	datasets: DatasetManifest[];
	count: number;
	root: string;
}

interface RustDatasetManifest {
	name: string;
	version: string;
	source?: string;
	total_examples: number;
	train_examples: number;
	eval_examples: number;
	train_path: string;
	eval_path: string;
	metrics?: { avg_cyclomatic_complexity?: number; avg_lines_of_code?: number };
	pre_cutoff?: number;
	post_cutoff?: number;
	imported_at: string;
}

// ============================================================================
// Helpers
// ============================================================================

function mapManifest(r: RustDatasetManifest): DatasetManifest {
	return {
		name: r.name,
		version: r.version,
		source: r.source,
		totalExamples: r.total_examples,
		trainExamples: r.train_examples,
		evalExamples: r.eval_examples,
		trainPath: r.train_path,
		evalPath: r.eval_path,
		metrics: r.metrics ? {
			avgCyclomaticComplexity: r.metrics.avg_cyclomatic_complexity,
			avgLinesOfCode: r.metrics.avg_lines_of_code,
		} : undefined,
		preCutoff: r.pre_cutoff,
		postCutoff: r.post_cutoff,
		importedAt: r.imported_at,
	};
}

// ============================================================================
// Mixin
// ============================================================================

export interface DatasetMixin {
	datasetImportCsv(params: {
		csvPath: string;
		name?: string;
		outputDir?: string;
		splitRatio?: number;
		userColumn?: string;
		assistantColumn?: string;
	}): Promise<DatasetManifest>;

	datasetImportRealClassEval(params: {
		repoDir?: string;
		csvPath?: string;
		testsDir?: string;
		outputDir?: string;
		splitRatio?: number;
	}): Promise<DatasetManifest>;

	datasetList(outputDir?: string): Promise<DatasetListResult>;

	datasetInfo(name: string, outputDir?: string): Promise<DatasetManifest>;
}

export function DatasetMixin<T extends new (...args: any[]) => RustCoreIPCClientBase>(Base: T) {
	return class extends Base implements DatasetMixin {
		/**
		 * Import a generic CSV file as a JSONL training dataset.
		 */
		async datasetImportCsv(params: {
			csvPath: string;
			name?: string;
			outputDir?: string;
			splitRatio?: number;
			userColumn?: string;
			assistantColumn?: string;
		}): Promise<DatasetManifest> {
			const response = await this.request({ command: 'dataset/import-csv', ...params });
			if (!response.success) throw new Error(response.error || 'dataset/import-csv failed');
			return mapManifest(response.result as RustDatasetManifest);
		}

		/**
		 * Import RealClassEval dataset from cloned repo or explicit CSV path.
		 * Prefer repoDir for auto-discovery of all splits (csn + post_cut-off).
		 */
		async datasetImportRealClassEval(params: {
			repoDir?: string;
			csvPath?: string;
			testsDir?: string;
			outputDir?: string;
			splitRatio?: number;
		}): Promise<DatasetManifest> {
			const response = await this.request({ command: 'dataset/import-realclasseval', ...params });
			if (!response.success) throw new Error(response.error || 'dataset/import-realclasseval failed');
			return mapManifest(response.result as RustDatasetManifest);
		}

		/**
		 * List all imported datasets.
		 */
		async datasetList(outputDir?: string): Promise<DatasetListResult> {
			const response = await this.request({
				command: 'dataset/list',
				...(outputDir ? { outputDir } : {}),
			});
			if (!response.success) throw new Error(response.error || 'dataset/list failed');
			const r = response.result as { datasets: RustDatasetManifest[]; count: number; root: string };
			return {
				datasets: r.datasets.map(mapManifest),
				count: r.count,
				root: r.root,
			};
		}

		/**
		 * Get manifest info for a specific dataset.
		 */
		async datasetInfo(name: string, outputDir?: string): Promise<DatasetManifest> {
			const response = await this.request({
				command: 'dataset/info',
				name,
				...(outputDir ? { outputDir } : {}),
			});
			if (!response.success) throw new Error(response.error || 'dataset/info failed');
			return mapManifest(response.result as RustDatasetManifest);
		}
	};
}
