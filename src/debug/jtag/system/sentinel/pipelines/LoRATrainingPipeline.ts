/**
 * LoRATrainingPipeline - Sentinel pipeline template for full LoRA training workflow
 *
 * Generates a Pipeline definition that orchestrates:
 * 1. Dataset preparation (genome/dataset-prepare)
 * 2. Condition check on success
 * 3. Training (genome/train)
 * 4. Adapter registration (genome/paging-adapter-register)
 * 5. Adapter activation (genome/paging-activate)
 *
 * Uses Rust interpolation {{steps.N.data.field}} for step-to-step data flow.
 */

import type { Pipeline, PipelineStep } from '../../../workers/continuum-core/bindings/modules/sentinel';
import type { UUID } from '../../core/types/CrossPlatformUUID';
import { v4 as uuidv4 } from 'uuid';

/**
 * Configuration for building a LoRA training pipeline
 */
export interface LoRATrainingConfig {
  personaId: UUID;
  personaName: string;
  roomId: UUID;
  traitType?: string;
  baseModel?: string;
  rank?: number;
  epochs?: number;
  learningRate?: number;
  batchSize?: number;
}

/**
 * Build a Sentinel pipeline definition for the full LoRA training workflow.
 *
 * Step flow:
 *   0: genome/dataset-prepare → produces { datasetPath, exampleCount }
 *   1: condition on step 0 success
 *     then:
 *       2: genome/train → produces { adapterPath, metrics }
 *       3: genome/paging-adapter-register → registers adapter
 *       4: genome/paging-activate → loads adapter for persona
 */
export function buildLoRATrainingPipeline(config: LoRATrainingConfig): Pipeline {
  const {
    personaId,
    personaName,
    roomId,
    traitType = 'conversational',
    baseModel = 'smollm2:135m',
    rank = 32,
    epochs = 3,
    learningRate = 0.0001,
    batchSize = 4,
  } = config;

  const adapterId = uuidv4() as UUID;
  const safeName = personaName.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const adapterName = `${safeName}-${traitType}`;

  const steps: PipelineStep[] = [
    // Step 0: Prepare training dataset from chat history
    {
      type: 'command',
      command: 'genome/dataset-prepare',
      params: {
        personaId,
        personaName,
        roomId,
        traitType,
      },
    },

    // Step 1: Check if dataset preparation succeeded, then train + register + activate
    {
      type: 'condition',
      if: '{{steps.0.data.success}}',
      then: [
        // Step 1.0 (nested): Train LoRA adapter
        {
          type: 'command',
          command: 'genome/train',
          params: {
            personaId,
            personaName,
            traitType,
            baseModel,
            datasetPath: '{{steps.0.data.datasetPath}}',
            rank,
            epochs,
            learningRate,
            batchSize,
          },
        },

        // Step 1.1 (nested): Register the trained adapter in genome registry
        // Uses layerId from train step to hydrate from persisted GenomeLayerEntity
        {
          type: 'command',
          command: 'genome/paging-adapter-register',
          params: {
            layerId: '{{steps.1.0.data.layerId}}',
            adapterId,
            name: adapterName,
            domain: traitType,
            sizeMB: 0, // Overridden by entity lookup when layerId is available
          },
        },

        // Step 1.2 (nested): Activate the adapter for the persona
        {
          type: 'command',
          command: 'genome/paging-activate',
          params: {
            personaId,
            adapterId,
          },
        },
      ],
    },
  ];

  return {
    name: `lora-training-${safeName}`,
    steps,
    inputs: {
      personaId,
      personaName,
      roomId,
      traitType,
      baseModel,
    },
  };
}
