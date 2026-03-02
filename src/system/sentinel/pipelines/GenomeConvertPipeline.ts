/**
 * GenomeConvertPipeline - Sentinel pipeline for adapter format conversion
 *
 * Orchestrates the conversion of LoRA adapters between formats:
 * 1. Read source adapter manifest (genome layer entity)
 * 2. Determine conversion direction based on quantization metadata
 * 3. Execute conversion via genome/convert command
 * 4. Register the converted output as a new genome layer
 * 5. Emit completion event
 *
 * Conversion directions:
 *   QLoRA adapter + FP16 base → merged GGUF (for M1 deployment)
 *   QLoRA adapter + FP16 base → merged FP16 safetensors (for 5090 deployment)
 *   HuggingFace model → GGUF (quantize base model only)
 *
 * The pipeline uses Rust interpolation {{steps.N.data.field}} for step-to-step data flow
 * and runs each conversion step through the Rust sentinel for process isolation.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { LOCAL_MODELS } from '@system/shared/Constants';
import { v4 as uuidv4 } from 'uuid';

/**
 * Configuration for building a genome conversion pipeline
 */
export interface GenomeConvertConfig {
  /** Source adapter path (required for merge operations) */
  adapterPath?: string;
  /** Base model name or HuggingFace ID */
  baseModel?: string;
  /** Target operation: 'merge-full', 'merge-and-quantize', 'quantize-base' */
  operation: 'merge-full' | 'merge-and-quantize' | 'quantize-base';
  /** Quantization bits for GGUF output (default: 4) */
  bits?: 4 | 8;
  /** Output directory override */
  outputPath?: string;
  /** Run validation inference after conversion (default: true) */
  validate?: boolean;
  /** Persona ID to activate the converted adapter for */
  personaId?: UUID;
  /** Adapter domain for registration (e.g., 'coding', 'conversational') */
  domain?: string;
}

/**
 * Build a Sentinel pipeline for adapter format conversion.
 *
 * Step flow:
 *   0: genome/convert → produces { outputPath, format, sizeMB, durationSeconds, compressionRatio, validation }
 *   1: condition on step 0 success
 *     then:
 *       1.0: genome/paging-adapter-register → registers converted adapter
 *       1.1: (optional) genome/paging-activate → activates for persona
 *   2: emit genome:conversion:complete
 */
export function buildGenomeConvertPipeline(config: GenomeConvertConfig): Pipeline {
  const {
    adapterPath,
    baseModel = LOCAL_MODELS.DEFAULT,
    operation,
    bits = 4,
    outputPath,
    validate = true,
    personaId,
    domain = 'general',
  } = config;

  const adapterId = uuidv4() as UUID;
  const pipelineName = `genome-convert-${operation}`;

  const steps: PipelineStep[] = [
    // Step 0: Execute the conversion
    {
      type: 'command',
      command: 'genome/convert',
      params: {
        operation,
        adapterPath: adapterPath ?? '',
        baseModel,
        bits,
        outputPath: outputPath ?? '',
        validate,
      },
    },

    // Step 1: On success, register + optionally activate
    {
      type: 'condition',
      if: '{{steps.0.data.success}}',
      then: buildPostConversionSteps(adapterId, domain, personaId),
      else: [
        // Emit failure event
        {
          type: 'emit',
          event: 'genome:conversion:failed',
          payload: {
            operation,
            adapterPath: adapterPath ?? '',
            error: '{{steps.0.data.error}}',
          },
        },
      ],
    },

    // Step 2: Emit completion event
    {
      type: 'emit',
      event: 'genome:conversion:complete',
      payload: {
        operation,
        outputPath: '{{steps.0.data.outputPath}}',
        format: '{{steps.0.data.format}}',
        sizeMB: '{{steps.0.data.sizeMB}}',
        durationSeconds: '{{steps.0.data.durationSeconds}}',
        compressionRatio: '{{steps.0.data.compressionRatio}}',
        adapterId,
      },
    },
  ];

  return {
    name: pipelineName,
    steps,
    inputs: {
      operation,
      adapterPath: adapterPath ?? '',
      baseModel,
      bits,
      domain,
      personaId: personaId ?? '',
    },
  };
}

/**
 * Build post-conversion steps: register adapter + optional activation
 */
function buildPostConversionSteps(adapterId: UUID, domain: string, personaId?: UUID): PipelineStep[] {
  const steps: PipelineStep[] = [
    // Step 1.0: Register the converted output as a genome layer
    {
      type: 'command',
      command: 'genome/paging-adapter-register',
      params: {
        adapterId,
        name: `converted-${domain}`,
        domain,
        modelPath: '{{steps.0.data.outputPath}}',
        sizeMB: '{{steps.0.data.sizeMB}}',
      },
    },
  ];

  // Step 1.1: Activate for persona if personaId provided
  if (personaId) {
    steps.push({
      type: 'command',
      command: 'genome/paging-activate',
      params: {
        personaId,
        adapterId,
      },
    });
  }

  return steps;
}

/**
 * Build a pipeline that trains with QLoRA then converts to GGUF.
 *
 * This is the Academy integration pattern:
 *   Teacher sentinel trains with QLoRA → on success → convert to GGUF-merged
 *   Both the original QLoRA adapter AND the GGUF-merged version are registered.
 *
 * Step flow:
 *   0: sentinel (nested LoRA training pipeline)
 *   1: watch for training completion event
 *   2: genome/convert (merge-and-quantize)
 *   3: condition → register + activate
 *   4: emit
 */
export function buildTrainAndConvertPipeline(
  trainingPipeline: Pipeline,
  config: Omit<GenomeConvertConfig, 'operation'>
): Pipeline {
  const convertConfig: GenomeConvertConfig = {
    ...config,
    operation: 'merge-and-quantize',
  };

  const convertPipeline = buildGenomeConvertPipeline(convertConfig);

  const steps: PipelineStep[] = [
    // Step 0: Run the training pipeline as a nested sentinel
    {
      type: 'sentinel',
      pipeline: trainingPipeline,
    },

    // Step 1: Execute the conversion pipeline as a nested sentinel
    {
      type: 'sentinel',
      pipeline: convertPipeline,
    },
  ];

  return {
    name: `train-and-convert-${config.domain ?? 'general'}`,
    steps,
    inputs: {
      ...trainingPipeline.inputs,
      convertBits: config.bits ?? 4,
    },
  };
}
