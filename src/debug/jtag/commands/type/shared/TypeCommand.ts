/**
 * Type Command - Abstract Base Class
 * 
 * Perfect example of the command pattern - minimal abstraction with proper
 * generics and clean inheritance. Follows screenshot/navigate/click examples exactly.
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { type TypeParams, type TypeResult, createTypeParams } from './TypeTypes';

export abstract class TypeCommand extends CommandBase<TypeParams, TypeResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('type', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): TypeParams {
    return createTypeParams(this.context, sessionId, {
      selector: 'input',
      text: '',
      clearFirst: false,
      delay: 0
    });
  }

  abstract execute(params: TypeParams): Promise<TypeResult>;
}