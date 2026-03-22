/**
 * Persona Allocate Command - Server Implementation
 *
 * Proxies to Rust PersonaAllocatorModule via Unix socket IPC.
 * Returns hardware-aware persona allocation decisions based on GPU VRAM
 * and available API keys. Single source of truth for persona creation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaAllocateParams, PersonaAllocateResult } from '../shared/PersonaAllocateTypes';
import { createPersonaAllocateResultFromParams } from '../shared/PersonaAllocateTypes';
import { RustCoreIPCClient, getContinuumCoreSocketPath } from '../../../../workers/continuum-core/bindings/RustCoreIPC';

export class PersonaAllocateServerCommand extends CommandBase<PersonaAllocateParams, PersonaAllocateResult> {
  private rustClient: RustCoreIPCClient;

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/allocate', context, subpath, commander);
    this.rustClient = new RustCoreIPCClient(getContinuumCoreSocketPath());
  }

  async execute(params: PersonaAllocateParams): Promise<PersonaAllocateResult> {
    await this.rustClient.connect();

    try {
      const response = await this.rustClient.request({
        command: 'persona/allocate',
        availableApiKeys: params.availableApiKeys || [],
      });

      if (!response.success) {
        throw new Error(response.error || 'Rust persona/allocate failed');
      }

      const r = response.result as any;

      return createPersonaAllocateResultFromParams(params, {
        success: true,
        allocations: r.allocations || [],
        skipped: r.skipped || [],
        summary: r.summary || [],
        gpuName: r.gpu_name || r.gpuName || '',
        totalVramGb: r.total_vram_gb || r.totalVramGb || 0,
        gpuType: r.gpu_type || r.gpuType || 'cpu',
        localModel: r.local_model || r.localModel || '',
      });
    } finally {
      this.rustClient.disconnect();
    }
  }
}
