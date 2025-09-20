/**
 * User Create Persona Command - Browser Implementation
 *
 * Delegates to server for actual PersonaUser creation via UserDaemonServer
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { UserCreatePersonaParams, UserCreatePersonaResult } from '../shared/UserCreatePersonaTypes';

export class UserCreatePersonaBrowserCommand extends CommandBase<UserCreatePersonaParams, UserCreatePersonaResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('user-create-persona', context, subpath, commander);
  }

  async execute(params: UserCreatePersonaParams): Promise<UserCreatePersonaResult> {
    console.debug(`🎭 USER BROWSER: Delegating PersonaUser creation "${params.displayName}" (${params.personaStyle}) to server`);

    // Delegate to server implementation
    return await this.remoteExecute(params);
  }
}