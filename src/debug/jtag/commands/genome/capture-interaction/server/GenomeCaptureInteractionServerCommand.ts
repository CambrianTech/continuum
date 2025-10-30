/**
 * GenomeCaptureInteractionServerCommand - Captures AI interactions for training
 *
 * Records inputs/outputs during recipe execution for continuous learning.
 * Stores in PersonaUser's TrainingDataAccumulator for batch micro-tuning.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeCaptureInteractionParams,
  GenomeCaptureInteractionResult
} from '../shared/GenomeCaptureInteractionTypes';
import { v4 as uuidv4 } from 'uuid';
import type { UUID } from '../../../../system/core/types/CrossPlatformUUID';

export class GenomeCaptureInteractionServerCommand extends CommandBase<
  GenomeCaptureInteractionParams,
  GenomeCaptureInteractionResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-capture-interaction', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeCaptureInteractionResult> {
    const captureParams = params as GenomeCaptureInteractionParams;

    console.log('🧬 GENOME CAPTURE: Recording interaction');
    console.log(`   Role: ${captureParams.roleId}`);
    console.log(`   Domain: ${captureParams.domain}`);
    console.log(`   Input length: ${captureParams.input.length}`);
    console.log(`   Output length: ${captureParams.output.length}`);

    try {
      // Get PersonaUser's training accumulator
      // TODO: Access PersonaUser instance and its TrainingDataAccumulator
      // For now, we'll store in a temporary in-memory structure

      const exampleId = uuidv4() as UUID;

      // TODO: Replace with actual PersonaUser.trainingAccumulator.captureInteraction()
      // This is a placeholder implementation
      const capture = {
        exampleId,
        roleId: captureParams.roleId,
        domain: captureParams.domain,
        loraAdapter: captureParams.loraAdapter ?? `${captureParams.domain}-base`,
        input: captureParams.input,
        output: captureParams.output,
        thoughtStream: captureParams.thoughtStream,
        metadata: {
          ...captureParams.metadata,
          capturedAt: new Date().toISOString()
        }
      };

      // Store capture (temporary - should go to PersonaUser's accumulator)
      console.log(`📝 Captured interaction ${exampleId.slice(0, 8)}... for ${captureParams.domain}`);

      // TODO: Get actual buffer size from TrainingDataAccumulator
      const bufferSize = 1; // Placeholder
      const batchThreshold = 10;
      const readyForTraining = bufferSize >= batchThreshold;

      if (readyForTraining) {
        console.log(`✅ Buffer ready for training (${bufferSize}/${batchThreshold} examples)`);
      }

      return transformPayload(params, {
        success: true,
        capture: {
          exampleId,
          domain: captureParams.domain,
          roleId: captureParams.roleId,
          bufferSize,
          readyForTraining
        }
      });

    } catch (error) {
      console.error('❌ GENOME CAPTURE: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error)
      });
    }
  }
}
