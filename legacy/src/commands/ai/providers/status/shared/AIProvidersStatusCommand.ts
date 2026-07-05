/**
 * AI Providers Status Command - Shared Logic
 *
 * Base class for browser and server implementations.
 */

import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { AIProvidersStatusParams, AIProvidersStatusResult } from './AIProvidersStatusTypes';

export abstract class AIProvidersStatusCommand extends CommandBase<
  AIProvidersStatusParams,
  AIProvidersStatusResult
> {
  constructor(
    name: string,
    context: AIProvidersStatusParams['context'],
    subpath: string,
    commander: ICommandDaemon
  ) {
    super(name, context, subpath, commander);
  }
}
