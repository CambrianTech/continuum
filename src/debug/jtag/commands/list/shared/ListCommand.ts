import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';
import { type ListParams, type ListResult, createListParams } from './ListTypes';

export abstract class ListCommand extends CommandBase<ListParams, ListResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('list', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ListParams {
    return createListParams(this.context, sessionId, {
      category: 'all',
      includeDescription: true,
      includeSignature: true
    });
  }

  abstract execute(params: ListParams): Promise<ListResult>;
}