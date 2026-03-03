/**
 * Genome Convert Command - Server Implementation
 *
 * Converts LoRA adapters between formats via Python subprocess (convert-adapter.py).
 * Routes through Rust sentinel for process isolation, timeout enforcement, and log capture.
 *
 * Operations:
 *   merge-full          Merge LoRA into FP16 safetensors (for 5090 deployment)
 *   merge-and-quantize  Merge LoRA + quantize to GGUF (for M1/resource-constrained)
 *   quantize-base       Convert HF model to GGUF (no LoRA)
 *   validate            Sanity-check a converted model
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { GenomeConvertParams, GenomeConvertResult } from '../shared/GenomeConvertTypes';
import { createGenomeConvertResultFromParams } from '../shared/GenomeConvertTypes';
import { RustCoreIPCClient } from '../../../../workers/continuum-core/bindings/RustCoreIPC';
import { LOCAL_MODELS } from '@system/shared/Constants';
import { AdapterStore } from '@system/genome/server/AdapterStore';
import * as fs from 'fs';
import * as path from 'path';

const VALID_OPERATIONS = ['merge-full', 'merge-and-quantize', 'quantize-base', 'validate'] as const;
type ConvertOperation = typeof VALID_OPERATIONS[number];

export class GenomeConvertServerCommand extends CommandBase<GenomeConvertParams, GenomeConvertResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome/convert', context, subpath, commander);
  }

  async execute(params: GenomeConvertParams): Promise<GenomeConvertResult> {
    // Validate operation
    const operation = params.operation as ConvertOperation;
    if (!VALID_OPERATIONS.includes(operation)) {
      throw new ValidationError(
        'operation',
        `Invalid operation '${params.operation}'. Must be one of: ${VALID_OPERATIONS.join(', ')}`
      );
    }

    // Validate operation-specific params
    if ((operation === 'merge-full' || operation === 'merge-and-quantize') && !params.adapterPath) {
      throw new ValidationError('adapterPath', `'adapterPath' is required for '${operation}' operation`);
    }

    if (operation !== 'validate' && !params.baseModel) {
      throw new ValidationError('baseModel', `'baseModel' is required for '${operation}' operation`);
    }

    if (params.adapterPath && !AdapterStore.isValidAdapterPath(params.adapterPath)) {
      throw new ValidationError('adapterPath', `Invalid adapter path: ${params.adapterPath}`);
    }

    // Resolve output path
    const outputPath = params.outputPath || this.defaultOutputPath(operation, params);
    const bits = params.bits ?? 4;
    const shouldValidate = params.validate !== false; // Default: true

    // Map base model name
    const baseModel = params.baseModel ? LOCAL_MODELS.mapToHuggingFace(params.baseModel) : '';

    // Check GPU pressure before conversion (conversions are memory-intensive)
    const rustClient = RustCoreIPCClient.getInstance();
    try {
      const stats = await rustClient.gpuStats();
      console.log(`[genome/convert] GPU: ${stats.gpuName}, pressure=${(stats.pressure * 100).toFixed(1)}%, inference=${stats.inference.usedMb}/${stats.inference.budgetMb}MB`);

      if (stats.pressure > 0.8 && operation !== 'validate') {
        throw new Error(
          `GPU pressure too high for conversion (${(stats.pressure * 100).toFixed(1)}%). ` +
          `Free GPU memory or wait for current operations to complete. ` +
          `VRAM: ${stats.totalUsedMb}/${stats.totalVramMb}MB`
        );
      }
    } catch (err) {
      // GPU stats unavailable — log and proceed (conversion uses CPU via Python)
      if (err instanceof Error && err.message.includes('GPU pressure too high')) {
        throw err;
      }
      console.warn(`[genome/convert] GPU stats unavailable: ${err}`);
    }

    console.log(`[genome/convert] ${operation}: adapter=${params.adapterPath || 'N/A'}, base=${baseModel}, bits=${bits}, output=${outputPath}`);

    // Build Python script arguments
    const scriptPath = this.getConvertScriptPath();
    const wrapperPath = this.getPythonWrapperPath();

    if (!fs.existsSync(wrapperPath)) {
      throw new Error(
        `Training environment not bootstrapped.\n` +
        `Run: bash .continuum/genome/python/bootstrap.sh`
      );
    }

    const args = [scriptPath, operation];
    if (params.adapterPath) args.push('--adapter', params.adapterPath);
    if (baseModel) args.push('--base', baseModel);
    if (operation.includes('quantize')) args.push('--bits', String(bits));
    args.push('--output', outputPath);

    // Execute via Rust sentinel for process isolation
    const result = await rustClient.sentinelExecute({
      command: wrapperPath,
      args,
      workingDir: process.cwd(),
      timeout: 1800, // 30 minutes for large model conversions
      type: 'conversion',
    });

    if (!result.success) {
      const errorLines = result.output.split('\n')
        .filter((line: string) => line.includes('Error') || line.includes('Traceback') || line.includes('❌'))
        .join('\n');
      throw new Error(
        `Conversion failed (exit code ${result.exitCode})\n` +
        `Handle: ${result.handle}\n` +
        `${errorLines || result.output.slice(-500)}`
      );
    }

    // Read conversion result JSON
    const resultJsonPath = path.join(outputPath, 'conversion_result.json');
    let conversionResult: Record<string, unknown> = {};
    if (fs.existsSync(resultJsonPath)) {
      conversionResult = JSON.parse(fs.readFileSync(resultJsonPath, 'utf-8'));
    }

    // Optionally run validation
    let validation: Record<string, unknown> | undefined;
    if (shouldValidate && operation !== 'validate') {
      try {
        const modelPath = (conversionResult.outputPath as string) || outputPath;
        const valArgs = [scriptPath, 'validate', '--model', modelPath, '--output', outputPath];
        const valResult = await rustClient.sentinelExecute({
          command: wrapperPath,
          args: valArgs,
          workingDir: process.cwd(),
          timeout: 300,
          type: 'validation',
        });
        if (valResult.success && fs.existsSync(resultJsonPath)) {
          validation = JSON.parse(fs.readFileSync(resultJsonPath, 'utf-8'));
        }
      } catch (err) {
        console.warn(`[genome/convert] Validation failed (non-blocking): ${err}`);
        validation = { valid: false, error: String(err) };
      }
    }

    return createGenomeConvertResultFromParams(params, {
      success: true,
      outputPath: (conversionResult.outputPath as string) || outputPath,
      format: (conversionResult.format as string) || `gguf-q${bits}_0`,
      sizeMB: (conversionResult.sizeMB as number) || 0,
      durationSeconds: (conversionResult.durationSeconds as number) || 0,
      compressionRatio: (conversionResult.compressionRatio as number) || 0,
      validation: validation || {},
    });
  }

  private getConvertScriptPath(): string {
    // From commands/genome/convert/server/ → system/genome/fine-tuning/server/adapters/scripts/
    return path.resolve(__dirname, '../../../../system/genome/fine-tuning/server/adapters/scripts/convert-adapter.py');
  }

  private getPythonWrapperPath(): string {
    return path.join(process.cwd(), '.continuum', 'genome', 'python', 'train-wrapper.sh');
  }

  private defaultOutputPath(operation: ConvertOperation, params: GenomeConvertParams): string {
    const convertedDir = path.join('.continuum', 'genome', 'converted');
    fs.mkdirSync(convertedDir, { recursive: true });

    const timestamp = Date.now();
    const suffix = operation === 'merge-full' ? 'fp16' : `q${params.bits ?? 4}`;
    const base = params.adapterPath
      ? path.basename(params.adapterPath)
      : (params.baseModel ?? 'model').replace(/[^a-zA-Z0-9]/g, '-');

    return path.join(convertedDir, `${base}-${suffix}-${timestamp}`);
  }
}
