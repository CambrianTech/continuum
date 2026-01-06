/**
 * Persona Genome Command - Browser Implementation
 *
 * Get persona genome information including base model, layers, and traits
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaGenomeParams, PersonaGenomeResult } from '../shared/PersonaGenomeTypes';

export class PersonaGenomeBrowserCommand extends CommandBase<PersonaGenomeParams, PersonaGenomeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/genome', context, subpath, commander);
  }

  async execute(params: PersonaGenomeParams): Promise<PersonaGenomeResult> {
    console.log('🌐 BROWSER: Delegating Persona Genome to server');
    return await this.remoteExecute(params);
  }
}
