/**
 * BaseServerLoRATrainer - Base class for server-side LoRA trainers
 *
 * Extends BaseLoRATrainer with Node.js-specific utilities like:
 * - File system operations
 * - Path resolution
 * - Process spawning
 *
 * SERVER-ONLY: Uses Node.js APIs (fs, path, child_process)
 */

import { BaseLoRATrainer } from '../shared/BaseLoRATrainer';
import { TrainingDatasetBuilder } from './TrainingDatasetBuilder';
import type {
  LoRATrainingRequest,
  TrainingDataset
} from '../shared/FineTuningTypes';
import { spawn, ChildProcess } from 'child_process';
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
   * From adapter location (src/debug/jtag/system/genome/fine-tuning/server/adapters),
   * navigate up to the project root (e.g., /path/to/project/continuum).
   *
   * Calculation: adapters → server → fine-tuning → genome → system → jtag → debug → src → continuum (9 levels)
   *
   * @protected
   */
  protected getProjectRoot(): string {
    // __dirname will be the compiled location in the server directory
    // From src/debug/jtag/system/genome/fine-tuning/server, go up 7 levels to reach project root
    return path.resolve(__dirname, '../../../../../../..');
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
    capabilities: ReturnType<BaseServerLoRATrainer['getFineTuningCapabilities']>
  ): Promise<string> {
    const config = {
      baseModel: request.baseModel,
      datasetPath: '', // Will be set by Python script
      rank: request.rank ?? capabilities.defaultRank,
      alpha: request.alpha ?? capabilities.defaultAlpha,
      epochs: request.epochs ?? capabilities.defaultEpochs,
      learningRate: request.learningRate ?? capabilities.defaultLearningRate,
      batchSize: request.batchSize ?? capabilities.defaultBatchSize,
      outputDir: '' // Will be set by Python script
    };

    const configPath = path.join(os.tmpdir(), `jtag-config-${Date.now()}.json`);
    await fs.promises.writeFile(configPath, JSON.stringify(config, null, 2), 'utf-8');

    console.log(`   Config written to: ${configPath}`);
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

    console.log(`   Dataset exported to: ${tempPath}`);
    return tempPath;
  }

  /**
   * Execute Python training script via wrapper
   *
   * Uses isolated conda environment via train-wrapper.sh
   *
   * @param scriptName Python script name (e.g., 'peft-train.py')
   * @param configPath Path to config JSON file
   * @param outputDir Output directory for trained model
   * @returns Training metrics
   * @protected
   */
  protected async executePythonScript(
    scriptName: string,
    configPath: string,
    outputDir: string
  ): Promise<{ finalLoss: number }> {
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

    console.log(`   Executing: ${wrapperPath} ${scriptPath}`);
    console.log(`   Config: ${configPath}`);
    console.log(`   Output: ${outputDir}`);

    return new Promise((resolve, reject) => {
      // Use wrapper script to run Python in isolated environment
      const python = spawn(wrapperPath, [scriptPath, '--config', configPath, '--output', outputDir]);

      let stderr = '';
      let finalLoss = 0.5; // Default

      python.stdout.on('data', (data: Buffer) => {
        const text = data.toString();
        process.stdout.write(text); // Stream to console

        // Parse final loss from output
        const lossMatch = text.match(/Final loss: ([\d.]+)/);
        if (lossMatch) {
          finalLoss = parseFloat(lossMatch[1]);
        }
      });

      python.stderr.on('data', (data: Buffer) => {
        const text = data.toString();
        stderr += text;
        process.stderr.write(text); // Stream to console
      });

      python.on('close', (code: number | null) => {
        if (code === 0) {
          console.log(`   Training script completed successfully`);
          resolve({ finalLoss });
        } else {
          reject(new Error(`Training script failed with exit code ${code}\nStderr: ${stderr}`));
        }
      });

      python.on('error', (error: Error) => {
        reject(new Error(`Failed to spawn Python process: ${error.message}`));
      });
    });
  }

  /**
   * Save trained adapter to genome storage
   *
   * @param request Training request (for naming)
   * @param outputDir Directory containing trained adapter files
   * @returns Path to saved adapter
   * @protected
   */
  protected async saveAdapter(request: LoRATrainingRequest, outputDir: string): Promise<string> {
    // Create genome adapters directory
    const adaptersDir = path.join('.continuum', 'genome', 'adapters');
    await fs.promises.mkdir(adaptersDir, { recursive: true });

    // Create adapter subdirectory
    const adapterName = `${request.personaName.replace(/\s+/g, '-')}-${request.traitType}-${Date.now()}`;
    const adapterPath = path.join(adaptersDir, adapterName);
    await fs.promises.mkdir(adapterPath, { recursive: true });

    // Copy all adapter files from output directory
    const files = await fs.promises.readdir(outputDir);
    for (const file of files) {
      const srcPath = path.join(outputDir, file);
      const destPath = path.join(adapterPath, file);
      await fs.promises.copyFile(srcPath, destPath);
    }

    console.log(`   Adapter files copied to: ${adapterPath}`);
    return adapterPath;
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
        console.log(`   Cleaned up: ${filePath}`);
      } catch (error) {
        console.warn(`   Failed to clean up ${filePath}:`, error);
      }
    }
  }
}
