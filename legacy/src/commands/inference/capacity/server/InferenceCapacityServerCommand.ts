/**
 * Inference Capacity Command - Server Implementation
 *
 * Report local-inference concurrency cap. How many parallel generate requests the hardware can handle simultaneously — matches the BatchScheduler's n_seq_max and the InferenceCoordinator's admission slots. Scaled by RAM: 48GB+ → 3, 16GB+ → 2, else 1. Single source of truth across the TS admission layer and the Rust scheduler (see issue #887).
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { InferenceCapacityParams, InferenceCapacityResult } from '../shared/InferenceCapacityTypes';
import { createInferenceCapacityResultFromParams } from '../shared/InferenceCapacityTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class InferenceCapacityServerCommand extends CommandBase<InferenceCapacityParams, InferenceCapacityResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('inference/capacity', context, subpath, commander);
  }

  async execute(params: InferenceCapacityParams): Promise<InferenceCapacityResult> {
    // Thin wrapper over the Rust-side InferenceModule IPC handler —
    // single source of truth lives at continuum-core's
    // `system_resources::local_inference_capacity()`. See issue #887.
    const capacity = await RustCoreIPCClient.getInstance().inferenceCapacity();

    return createInferenceCapacityResultFromParams(params, {
      success: true,
      capacity,
    });
  }
}
