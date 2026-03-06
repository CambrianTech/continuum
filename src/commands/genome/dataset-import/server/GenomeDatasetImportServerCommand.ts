/**
 * Genome Dataset Import Command - Server Implementation
 *
 * Thin wrapper that delegates all heavy I/O to the Rust DatasetModule.
 * Supports 'csv' (generic) and 'realclasseval' (RealClassEval benchmark) sources.
 *
 * For RealClassEval: auto-downloads the dataset if csvPath is not provided.
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';
import { SystemPaths } from '../../../../system/core/config/SystemPaths';
import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeDatasetImportParams, GenomeDatasetImportResult } from '../shared/GenomeDatasetImportTypes';
import { createGenomeDatasetImportResultFromParams } from '../shared/GenomeDatasetImportTypes';
import RustCoreIPCClient from '../../../../workers/continuum-core/bindings/RustCoreIPC';

/** Standard location for raw RealClassEval download */
const REALCLASSEVAL_RAW_DIR = path.join(SystemPaths.datasets.root, 'realclasseval-raw');
/** Standard location for imported (JSONL) dataset */
const REALCLASSEVAL_IMPORTED_DIR = path.join(SystemPaths.datasets.root, 'realclasseval');
const REALCLASSEVAL_REPO = 'https://github.com/mrsumitbd/RealClassEval-Replication';

export class GenomeDatasetImportServerCommand extends CommandBase<GenomeDatasetImportParams, GenomeDatasetImportResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/dataset-import', context, subpath, commander);
  }

  async execute(params: GenomeDatasetImportParams): Promise<GenomeDatasetImportResult> {
    // Handle --list mode
    if (params.list) {
      return this.handleList(params);
    }

    const { source } = params;

    if (!source) {
      throw new ValidationError('source', 'Missing required parameter. Use --source="csv" or --source="realclasseval".');
    }

    const client = await RustCoreIPCClient.getInstanceAsync();

    if (source === 'realclasseval') {
      return this.importRealClassEval(params, client);
    } else {
      if (!params.csvPath) {
        throw new ValidationError('csvPath', 'Missing required parameter. Path to CSV file.');
      }
      console.log(`📦 DATASET IMPORT [${source}]: ${params.csvPath}`);
      return this.importCsv(params, client);
    }
  }

  private async importRealClassEval(
    params: GenomeDatasetImportParams,
    client: RustCoreIPCClient,
  ): Promise<GenomeDatasetImportResult> {
    // If explicit csvPath provided, use legacy single-CSV mode
    if (params.csvPath) {
      console.log(`📦 DATASET IMPORT [realclasseval]: ${params.csvPath}`);
      const outputDir = params.outputDir ?? REALCLASSEVAL_IMPORTED_DIR;
      const manifest = await client.datasetImportRealClassEval({
        csvPath: params.csvPath,
        testsDir: params.testsDir,
        outputDir,
        splitRatio: params.splitRatio,
      });
      return this.buildRealClassEvalResult(params, manifest, outputDir);
    }

    // Auto-download repo if not present
    if (!fs.existsSync(REALCLASSEVAL_RAW_DIR)) {
      console.log('📦 DATASET IMPORT: RealClassEval not found locally, downloading...');
      this.downloadRealClassEval();
    }

    // Verify repo structure exists
    const dataDir = path.join(REALCLASSEVAL_RAW_DIR, 'data', 'functional_correctness_data');
    if (!fs.existsSync(dataDir)) {
      throw new ValidationError('repoDir', `Auto-download completed but expected directory not found at ${dataDir}. Repository structure may have changed.`);
    }

    console.log(`📦 DATASET IMPORT [realclasseval]: ${REALCLASSEVAL_RAW_DIR}`);

    const outputDir = params.outputDir ?? REALCLASSEVAL_IMPORTED_DIR;

    const manifest = await client.datasetImportRealClassEval({
      repoDir: REALCLASSEVAL_RAW_DIR,
      outputDir,
      splitRatio: params.splitRatio,
    });

    return this.buildRealClassEvalResult(params, manifest, outputDir);
  }

  private buildRealClassEvalResult(
    params: GenomeDatasetImportParams,
    manifest: Awaited<ReturnType<RustCoreIPCClient['datasetImportRealClassEval']>>,
    outputDir: string,
  ): GenomeDatasetImportResult {
    console.log(`✅ DATASET IMPORT: ${manifest.totalExamples} examples (${manifest.trainExamples} train, ${manifest.evalExamples} eval)`);

    return createGenomeDatasetImportResultFromParams(params, {
      success: true,
      name: manifest.name,
      totalExamples: manifest.totalExamples,
      trainExamples: manifest.trainExamples,
      evalExamples: manifest.evalExamples,
      trainPath: path.join(outputDir, manifest.trainPath),
      evalPath: path.join(outputDir, manifest.evalPath),
      manifestPath: path.join(outputDir, 'manifest.json'),
      source: manifest.source,
    });
  }

  private async importCsv(
    params: GenomeDatasetImportParams,
    client: RustCoreIPCClient,
  ): Promise<GenomeDatasetImportResult> {
    const manifest = await client.datasetImportCsv({
      csvPath: params.csvPath!,
      name: params.name,
      outputDir: params.outputDir,
      splitRatio: params.splitRatio,
      userColumn: params.userColumn,
      assistantColumn: params.assistantColumn,
    });

    const outputDir = params.outputDir ?? SystemPaths.datasets.root;

    console.log(`✅ DATASET IMPORT: ${manifest.totalExamples} examples (${manifest.trainExamples} train, ${manifest.evalExamples} eval)`);

    return createGenomeDatasetImportResultFromParams(params, {
      success: true,
      name: manifest.name,
      totalExamples: manifest.totalExamples,
      trainExamples: manifest.trainExamples,
      evalExamples: manifest.evalExamples,
      trainPath: path.join(outputDir, manifest.trainPath),
      evalPath: path.join(outputDir, manifest.evalPath),
      manifestPath: path.join(outputDir, 'manifest.json'),
    });
  }

  private async handleList(params: GenomeDatasetImportParams): Promise<GenomeDatasetImportResult> {
    const client = await RustCoreIPCClient.getInstanceAsync();
    const result = await client.datasetList(params.outputDir);

    console.log(`📋 DATASETS: ${result.count} found in ${result.root}`);
    for (const ds of result.datasets) {
      console.log(`   ${ds.name}: ${ds.totalExamples} examples (${ds.trainExamples} train, ${ds.evalExamples} eval)${ds.source ? ` [${ds.source}]` : ''}`);
    }

    if (result.datasets.length > 0) {
      return createGenomeDatasetImportResultFromParams(params, {
        success: true,
        name: `${result.count} datasets found`,
        totalExamples: result.datasets.reduce((sum, ds) => sum + ds.totalExamples, 0),
        trainExamples: result.datasets.reduce((sum, ds) => sum + ds.trainExamples, 0),
        evalExamples: result.datasets.reduce((sum, ds) => sum + ds.evalExamples, 0),
        trainPath: '',
        evalPath: '',
        manifestPath: result.root,
      });
    }

    return createGenomeDatasetImportResultFromParams(params, {
      success: true,
      name: 'No datasets found',
      totalExamples: 0,
      trainExamples: 0,
      evalExamples: 0,
      trainPath: '',
      evalPath: '',
      manifestPath: result.root,
    });
  }

  /**
   * Download the RealClassEval replication repo via shallow git clone.
   * Blocks until complete — this is an on-demand acquisition step.
   */
  private downloadRealClassEval(): void {
    fs.mkdirSync(path.dirname(REALCLASSEVAL_RAW_DIR), { recursive: true });

    if (fs.existsSync(REALCLASSEVAL_RAW_DIR)) {
      console.log('   Pulling latest...');
      execSync(`cd "${REALCLASSEVAL_RAW_DIR}" && git pull --ff-only`, {
        stdio: 'inherit',
        timeout: 300_000,
      });
    } else {
      console.log(`   Cloning ${REALCLASSEVAL_REPO} (shallow)...`);
      execSync(`git clone --depth 1 "${REALCLASSEVAL_REPO}" "${REALCLASSEVAL_RAW_DIR}"`, {
        stdio: 'inherit',
        timeout: 300_000,
      });
    }

    console.log('   Download complete.');
  }
}
