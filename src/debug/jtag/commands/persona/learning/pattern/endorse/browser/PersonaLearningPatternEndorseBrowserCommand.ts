/**
 * Persona Learning Pattern Endorse Command - Browser Implementation
 *
 * Report the outcome of using a pattern. Updates confidence scores and can trigger validation or deprecation.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaLearningPatternEndorseParams, PersonaLearningPatternEndorseResult } from '../shared/PersonaLearningPatternEndorseTypes';

export class PersonaLearningPatternEndorseBrowserCommand extends CommandBase<PersonaLearningPatternEndorseParams, PersonaLearningPatternEndorseResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/pattern/endorse', context, subpath, commander);
  }

  async execute(params: PersonaLearningPatternEndorseParams): Promise<PersonaLearningPatternEndorseResult> {
    console.log('🌐 BROWSER: Delegating Persona Learning Pattern Endorse to server');
    return await this.remoteExecute(params);
  }
}
