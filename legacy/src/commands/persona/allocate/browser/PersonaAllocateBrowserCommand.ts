/**
 * Persona Allocate Command - Browser Implementation
 *
 * Hardware-aware persona allocation via Rust PersonaAllocator. Returns optimal persona assignments based on GPU VRAM and available API keys. Single source of truth for which personas should exist on this machine.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaAllocateParams, PersonaAllocateResult } from '../shared/PersonaAllocateTypes';

export class PersonaAllocateBrowserCommand extends CommandBase<PersonaAllocateParams, PersonaAllocateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/allocate', context, subpath, commander);
  }

  async execute(params: PersonaAllocateParams): Promise<PersonaAllocateResult> {
    console.log('🌐 BROWSER: Delegating Persona Allocate to server');
    return await this.remoteExecute(params);
  }
}
