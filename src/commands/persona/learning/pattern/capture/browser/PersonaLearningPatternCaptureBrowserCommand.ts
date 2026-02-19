/**
 * Persona Learning Pattern Capture Command - Browser Implementation
 *
 * Capture a successful pattern for cross-AI learning. When an AI discovers a working solution, they share it with the team.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { PersonaLearningPatternCaptureParams, PersonaLearningPatternCaptureResult } from '../shared/PersonaLearningPatternCaptureTypes';

export class PersonaLearningPatternCaptureBrowserCommand extends CommandBase<PersonaLearningPatternCaptureParams, PersonaLearningPatternCaptureResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('persona/learning/pattern/capture', context, subpath, commander);
  }

  async execute(params: PersonaLearningPatternCaptureParams): Promise<PersonaLearningPatternCaptureResult> {
    console.log('🌐 BROWSER: Delegating Persona Learning Pattern Capture to server');
    return await this.remoteExecute(params);
  }
}
