/**
 * Plasticity Topology Command - Server Implementation
 * Routes to Rust plasticity module via IPC.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import { ValidationError } from '@system/core/types/ErrorTypes';
import type { PlasticityTopologyParams, PlasticityTopologyResult } from '../shared/PlasticityTopologyTypes';
import { createPlasticityTopologyResultFromParams } from '../shared/PlasticityTopologyTypes';
import { RustCoreIPCClient } from '../../../../../core/continuum-core/bindings/RustCoreIPC';

export class PlasticityTopologyServerCommand extends CommandBase<PlasticityTopologyParams, PlasticityTopologyResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('plasticity/topology', context, subpath, commander);
  }

  async execute(params: PlasticityTopologyParams): Promise<PlasticityTopologyResult> {
    if (!params.topologyPath) {
      throw new ValidationError('topologyPath', 'Required: path to head_topology.json file');
    }

    const client = RustCoreIPCClient.getInstance();
    const result = await client.plasticityTopology({
      topologyPath: params.topologyPath,
    });

    return createPlasticityTopologyResultFromParams(params, {
      success: true,
      layers: result.layers,
      parameterReduction: result.parameterReduction,
      precisionProfile: result.precisionProfile,
    });
  }
}
