/**
 * BaseServerLoRATrainer - Base class for server-side LoRA trainers
 *
 * Extends BaseLoRATrainer with Node.js-specific utilities like:
 * - File system operations
 * - Path resolution
 * - Python subprocess execution via Rust sentinel (process isolation + management)
 *
 * SERVER-ONLY: Uses Node.js APIs (fs, path) and Rust IPC (RustCoreIPCClient)
 */

import { BaseLoRATrainer } from '../shared/BaseLoRATrainer';
import { TrainingDatasetBuilder } from './TrainingDatasetBuilder';
import type {
  LoRATrainingRequest,
  TrainingDataset
} from '../shared/FineTuningTypes';
import { AdapterPackage, type AdapterPackageManifest } from '../../server/AdapterPackage';
import type { TrainingMetadata } from '../../entities/GenomeLayerEntity';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import * as path from 'path';
import * as fs from 'fs';
import * as os from 'os';

/**
 * Base class for server-side LoRA trainers
 *
 * Provides common utilities for:
 * - File operations and path resolution
 * - Dataset export to JSONL
 * - Training config generation
 * - Python subprocess execution
 *
 * Subclasses should focus on provider-specific logic only.
 */
export abstract class BaseServerLoRATrainer extends BaseLoRATrainer {
  /**
   * Get the project root directory
   *
   * From adapter location (src/system/genome/fine-tuning/server/adapters),
   * navigate up to the project root (e.g., /path/to/project/continuum).
   *
   * Calculation: adapters → server → fine-tuning → genome → system → src → continuum (7 levels)
   *
   * @protected
   */
  protected getProjectRoot(): string {
    // __dirname will be the compiled location in the server directory
    // From src/system/genome/fine-tuning/server, go up 5 levels to reach project root
    return path.resolve(__dirname, '../../../../..');
  }

  /**
   * Get path to Python environment wrapper script
   *
   * @protected
   */
  protected getPythonWrapperPath(): string {
    const projectRoot = this.getProjectRoot();
    return path.join(projectRoot, '.continuum', 'genome', 'python', 'train-wrapper.sh');
  }

  /**
   * Check if Python environment is bootstrapped
   *
   * @protected
   */
  protected isPythonEnvironmentBootstrapped(): boolean {
    const wrapperPath = this.getPythonWrapperPath();
    return fs.existsSync(wrapperPath);
  }

  /**
   * Get path to Python training script
   *
   * @param scriptName Script name (e.g., 'peft-train.py')
   * @protected
   */
  protected getTrainingScriptPath(scriptName: string): string {
    // __dirname is at server/ level
    return path.join(__dirname, 'adapters', 'scripts', scriptName);
  }

  /**
   * Check if training script exists
   *
   * @param scriptName Script name (e.g., 'peft-train.py')
   * @protected
   */
  protected trainingScriptExists(scriptName: string): boolean {
    const scriptPath = this.getTrainingScriptPath(scriptName);
    return fs.existsSync(scriptPath);
  }

  /**
   * Create config JSON file for Python training script
   *
   * @param request Training configuration
   * @param capabilities Provider capabilities for defaults
   * @returns Path to created config file
   * @protected
   */
  protected async createConfigFile(
    request: LoRATrainingRequest,
    capabilities: ReturnType<BaseServerLoRATrainer['getFineTuningCapabilities']>,
    datasetPath?: string
  ): Promise<string> {
    const config = {
      baseModel: request.baseModel,
      datasetPath: datasetPath ?? '',
      rank: request.rank ?? capabilities.defaultRank,
      alpha: request.alpha ?? capabilities.defaultAlpha,
      epochs: request.epochs ?? capabilities.defaultEpochs,
      learningRate: request.learningRate ?? capabilities.defaultLearningRate,
      batchSize: request.batchSize ?? capabilities.defaultBatchSize,
      quantize: request.quantize ?? true,
      quantizeBits: request.quantizeBits ?? 4,
      outputDir: '' // Set by --output CLI arg
    };

    const configPath = path.join(os.tmpdir(), `jtag-config-${Date.now()}.json`);
    await fs.promises.writeFile(configPath, JSON.stringify(config, null, 2), 'utf-8');

    this.log('debug', `Config written to: ${configPath}`);
    return configPath;
  }

  /**
   * Export dataset to JSONL file
   *
   * @param dataset Training dataset
   * @returns Path to created JSONL file
   * @protected
   */
  protected async exportDatasetToJSONL(dataset: TrainingDataset): Promise<string> {
    const tempPath = path.join(os.tmpdir(), `jtag-training-${Date.now()}.jsonl`);
    const jsonl = TrainingDatasetBuilder.exportToJSONL(dataset);
    await fs.promises.writeFile(tempPath, jsonl, 'utf-8');

    this.log('debug', `Dataset exported to: ${tempPath}`);
    return tempPath;
  }

  /**
   * Execute Python training script via Rust sentinel process management.
   *
   * Routes the Python subprocess through Rust's SentinelModule which provides:
   * - kill_on_drop: automatic cleanup if sentinel is dropped
   * - Timeout enforcement at the Rust level
   * - Log capture to .sentinel-workspaces/{handle}/logs/
   * - Handle-based tracking: cancellable, status-queryable
   * - Concurrent execution management: Rust manages resource limits
   *
   * @param scriptName Python script name (e.g., 'peft-train.py')
   * @param configPath Path to config JSON file
   * @param outputDir Output directory for trained model
   * @param timeoutSecs Timeout in seconds (default: 600 = 10 minutes)
   * @returns Training metrics and sentinel handle
   * @protected
   */
  protected async executePythonScript(
    scriptName: string,
    configPath: string,
    outputDir: string,
    timeoutSecs: number = 600,
  ): Promise<{ finalLoss: number; handle: string }> {
    const scriptPath = this.getTrainingScriptPath(scriptName);
    const wrapperPath = this.getPythonWrapperPath();

    // Check if environment is bootstrapped
    if (!this.isPythonEnvironmentBootstrapped()) {
      throw new Error(
        `Training environment not bootstrapped.\n` +
        `Run: bash .continuum/genome/python/bootstrap.sh\n` +
        `This will install Python dependencies in an isolated environment.`
      );
    }

    this.log('info', `Executing training via Rust sentinel: script=${scriptPath}, config=${configPath}, output=${outputDir}`);

    // Route through Rust sentinel — process gets kill_on_drop, timeout, log capture
    const rustClient = RustCoreIPCClient.getInstance();
    const result = await rustClient.sentinelExecute({
      command: wrapperPath,
      args: [scriptPath, '--config', configPath, '--output', outputDir],
      workingDir: process.cwd(),
      timeout: timeoutSecs,
      type: 'training',
    });

    // Parse training output from sentinel logs
    const output = result.output;
    let finalLoss = 0.5; // Default

    // Parse final loss from captured output
    const lossMatch = output.match(/Final loss: ([\d.]+)/);
    if (lossMatch) {
      finalLoss = parseFloat(lossMatch[1]);
    }

    if (!result.success) {
      // Extract stderr-like content from combined log
      const errorLines = output.split('\n')
        .filter(line => line.includes('[stderr]') || line.includes('Error') || line.includes('Traceback'))
        .join('\n');
      throw new Error(
        `Training script failed with exit code ${result.exitCode}\n` +
        `Handle: ${result.handle} (logs at sentinel/logs/read --handle=${result.handle})\n` +
        `${errorLines || output.slice(-500)}`
      );
    }

    this.log('info', `Training script completed via sentinel (loss=${finalLoss}, handle=${result.handle})`);
    return { finalLoss, handle: result.handle };
  }

  /**
   * Save trained adapter to genome storage with manifest
   *
   * @param request Training request (for naming)
   * @param outputDir Directory containing trained adapter files
   * @param trainingMetadata Training provenance metadata
   * @returns Adapter path and manifest
   * @protected
   */
  protected async saveAdapter(
    request: LoRATrainingRequest,
    outputDir: string,
    trainingMetadata: TrainingMetadata,
  ): Promise<{ adapterPath: string; manifest: AdapterPackageManifest }> {
    // Create genome adapters directory
    const adaptersDir = path.join('.continuum', 'genome', 'adapters');
    await fs.promises.mkdir(adaptersDir, { recursive: true });

    // Create adapter subdirectory
    const adapterName = `${request.personaName.replace(/\s+/g, '-')}-${request.traitType}-${Date.now()}`;
    const adapterPath = path.join(adaptersDir, adapterName);
    await fs.promises.mkdir(adapterPath, { recursive: true });

    // Copy all adapter files from output directory (handles both files and subdirectories)
    await this.copyDirRecursive(outputDir, adapterPath);

    // Calculate real size and content hash
    const sizeMB = await AdapterPackage.calculateSizeMB(adapterPath);
    const contentHash = await AdapterPackage.calculateContentHash(adapterPath);

    // Build and write manifest
    const manifest = AdapterPackage.buildManifest({
      adapterPath,
      personaId: request.personaId,
      personaName: request.personaName,
      traitType: request.traitType,
      baseModel: request.baseModel,
      rank: request.rank ?? 32,
      sizeMB,
      contentHash,
      trainingMetadata,
    });

    await AdapterPackage.writeManifest(adapterPath, manifest);

    this.log('info', `Adapter saved: ${adapterPath} (${sizeMB}MB, hash=${contentHash.slice(0, 12)})`);
    return { adapterPath, manifest };
  }

  /**
   * Recursively copy a directory's contents to a destination
   */
  private async copyDirRecursive(src: string, dest: string): Promise<void> {
    const entries = await fs.promises.readdir(src, { withFileTypes: true });
    for (const entry of entries) {
      const srcPath = path.join(src, entry.name);
      const destPath = path.join(dest, entry.name);
      if (entry.isDirectory()) {
        await fs.promises.mkdir(destPath, { recursive: true });
        await this.copyDirRecursive(srcPath, destPath);
      } else {
        await fs.promises.copyFile(srcPath, destPath);
      }
    }
  }

  /**
   * Clean up temporary files
   *
   * @param paths Paths to clean up (files or directories)
   * @protected
   */
  protected async cleanupTempFiles(...paths: string[]): Promise<void> {
    for (const filePath of paths) {
      try {
        const stats = await fs.promises.stat(filePath);
        if (stats.isDirectory()) {
          await fs.promises.rm(filePath, { recursive: true, force: true });
        } else {
          await fs.promises.unlink(filePath);
        }
        this.log('debug', `Cleaned up: ${filePath}`);
      } catch (error) {
        this.log('warn', `Failed to clean up ${filePath}: ${error}`);
      }
    }
  }
}
