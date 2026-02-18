/**
 * Persona Learning Pattern Query Command - Browser Implementation
 *
 * Query the collective pattern knowledge base. Search for patterns that might help solve the current problem.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaLearningPatternQueryParams, PersonaLearningPatternQueryResult } from '../shared/PersonaLearningPatternQueryTypes';

export class PersonaLearningPatternQueryBrowserCommand extends CommandBase<PersonaLearningPatternQueryParams, PersonaLearningPatternQueryResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/pattern/query', context, subpath, commander);
  }

  async execute(params: PersonaLearningPatternQueryParams): Promise<PersonaLearningPatternQueryResult> {
    console.log('🌐 BROWSER: Delegating Persona Learning Pattern Query to server');
    return await this.remoteExecute(params);
  }
}
