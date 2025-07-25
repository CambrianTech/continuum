/**
 * Type Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows screenshot/navigate/click examples exactly.
 */

import { CommandBase } from '../../../shared/CommandBase';
import type { ICommandDaemon } from '../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import { TypeParams } from './TypeTypes';
import type { TypeResult } from './TypeTypes';

export abstract class TypeCommand extends CommandBase<TypeParams, TypeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('type', context, subpath, commander);
  }

  public override getDefaultParams(): TypeParams {
    return new TypeParams({
      selector: 'input',
      text: '',
      clearFirst: false,
      delay: 0
    });
  }

  abstract execute(params: TypeParams): Promise<TypeResult>;
}