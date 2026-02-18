/**
 * GenomeBatchMicroTuneServerCommand - Trigger LoRA micro-tuning from accumulated examples
 *
 * Accesses the PersonaUser's TrainingDataAccumulator, checks if enough examples
 * have accumulated for the requested domain, and triggers training via
 * PersonaTrainingManager. Supports forceUpdate to bypass threshold check.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type {
  GenomeBatchMicroTuneParams,
  GenomeBatchMicroTuneResult
} from '../shared/GenomeBatchMicroTuneTypes';
import { UserDaemonServer } from '@daemons/user-daemon/server/UserDaemonServer';
import { PersonaUser } from '@system/user/server/PersonaUser';

export class GenomeBatchMicroTuneServerCommand extends CommandBase<
  GenomeBatchMicroTuneParams,
  GenomeBatchMicroTuneResult
> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('genome-batch-micro-tune', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<GenomeBatchMicroTuneResult> {
    const tuneParams = params as GenomeBatchMicroTuneParams;
    const domain = tuneParams.domain;
    const forceUpdate = tuneParams.forceUpdate ?? false;

    console.log(`🧬 GENOME MICRO-TUNE: domain=${domain}, force=${forceUpdate}`);

    try {
      // 1. Get UserDaemon singleton
      const userDaemon = UserDaemonServer.getInstance();
      if (!userDaemon) {
        return transformPayload(params, {
          success: false,
          error: 'UserDaemon not initialized',
        });
      }

      // 2. Get PersonaUser instance
      const personaId = tuneParams.personaId ?? tuneParams.userId;
      if (!personaId) {
        return transformPayload(params, {
          success: false,
          error: 'No personaId or userId provided',
        });
      }

      const baseUser = userDaemon.getPersonaUser(personaId);
      if (!baseUser || !(baseUser instanceof PersonaUser)) {
        return transformPayload(params, {
          success: false,
          error: `PersonaUser not found: ${personaId}`,
        });
      }

      const personaUser = baseUser as PersonaUser;
      const accumulator = personaUser.trainingAccumulator;

      // 3. Check buffer readiness
      const bufferSize = accumulator.getBufferSize(domain);
      const batchThreshold = accumulator.getBatchThreshold(domain);

      if (!forceUpdate && !accumulator.shouldMicroTune(domain)) {
        console.log(`⏳ GENOME MICRO-TUNE: Buffer not ready (${bufferSize}/${batchThreshold})`);
        return transformPayload(params, {
          success: true,
          training: {
            domain,
            loraAdapter: tuneParams.loraAdapter ?? `${domain}-base`,
            examplesUsed: 0,
            examplesFiltered: 0,
            updateType: 'none',
          },
        });
      }

      // 4. Trigger training via PersonaTrainingManager
      //    forceDomain bypasses the threshold check for the specified domain
      const startTime = Date.now();
      await personaUser.trainingManager.checkTrainingReadiness(forceUpdate ? domain : undefined);
      const trainingTime = Date.now() - startTime;

      // 5. Get post-training stats (buffer should be consumed now)
      const postBufferSize = accumulator.getBufferSize(domain);
      const examplesUsed = bufferSize - postBufferSize;

      console.log(`✅ GENOME MICRO-TUNE: ${examplesUsed} examples consumed in ${trainingTime}ms`);

      return transformPayload(params, {
        success: true,
        training: {
          domain,
          loraAdapter: tuneParams.loraAdapter ?? `${domain}-base`,
          examplesUsed,
          examplesFiltered: 0,
          updateType: examplesUsed > 0 ? 'soft' : 'none',
          metrics: {
            trainingTime,
            averageQuality: 0, // Quality scoring is Phase 12
            diversityScore: 0,
          },
        },
      });
    } catch (error) {
      console.error('❌ GENOME MICRO-TUNE: Error:', error);
      return transformPayload(params, {
        success: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  }
}
