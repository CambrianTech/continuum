/**
 * GenomeCaptureInteractionServerCommand - Captures AI interactions for training
 *
 * Records inputs/outputs during recipe execution for continuous learning.
 * Stores in PersonaUser's TrainingDataAccumulator for batch micro-tuning.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';
import { transformPayload } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type {
  GenomeCaptureInteractionParams,
  GenomeCaptureInteractionResult
} from '../shared/GenomeCaptureInteractionTypes';
import { UserDaemonServer } from '@daemons/user-daemon/server/UserDaemonServer';
import { PersonaUser } from '@system/user/server/PersonaUser';
import type { InteractionCapture } from '@system/user/server/modules/TrainingDataAccumulator';

export class GenomeCaptureInteractionServerCommand extends CommandBase<
  GenomeCaptureInteractionParams,
  GenomeCaptureInteractionResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/capture-interaction', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeCaptureInteractionResult> {
    const captureParams = params as GenomeCaptureInteractionParams;

    console.log('🧬 GENOME CAPTURE: Recording interaction');
    console.log(`   Role: ${captureParams.roleId}`);
    console.log(`   Domain: ${captureParams.domain}`);
    console.log(`   Input length: ${captureParams.input.length}`);
    console.log(`   Output length: ${captureParams.output.length}`);

    try {
      // Get UserDaemon singleton
      const userDaemon = UserDaemonServer.getInstance();
      if (!userDaemon) {
        return transformPayload(params, {
          success: false,
          error: 'UserDaemon not initialized'
        });
      }

      // Get PersonaUser instance (if personaId provided)
      const personaId = captureParams.personaId;
      if (!personaId) {
        // No persona specified - just log and return success
        console.log(`ℹ️ No personaId specified, interaction not captured`);
        return transformPayload(params, {
          success: true,
          capture: {
            exampleId: 'no-persona',
            domain: captureParams.domain,
            roleId: captureParams.roleId,
            bufferSize: 0,
            readyForTraining: false
          }
        });
      }

      const baseUser = userDaemon.getPersonaUser(personaId);
      if (!baseUser || !(baseUser instanceof PersonaUser)) {
        return transformPayload(params, {
          success: false,
          error: `PersonaUser not found: ${personaId}`
        });
      }

      const personaUser = baseUser as PersonaUser;

      // Capture interaction in TrainingDataAccumulator
      const capture: InteractionCapture = {
        roleId: captureParams.roleId,
        personaId: captureParams.personaId,
        domain: captureParams.domain,
        input: captureParams.input,
        output: captureParams.output,
        contextMetadata: {
          ...captureParams.metadata,
          loraAdapter: captureParams.loraAdapter,
          thoughtStream: captureParams.thoughtStream,
          capturedAt: new Date().toISOString()
        }
      };

      const exampleId = await personaUser.trainingAccumulator.captureInteraction(capture);

      // Get buffer stats
      const bufferSize = personaUser.trainingAccumulator.getBufferSize(captureParams.domain);
      const batchThreshold = personaUser.trainingAccumulator.getBatchThreshold(captureParams.domain);
      const readyForTraining = personaUser.trainingAccumulator.shouldMicroTune(captureParams.domain);

      if (readyForTraining) {
        console.log(`✅ ${personaUser.displayName}: Buffer ready for training (${bufferSize}/${batchThreshold} examples)`);
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
