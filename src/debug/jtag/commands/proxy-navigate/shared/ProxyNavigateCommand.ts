/**
 * Proxy Navigate Command - Abstract Base
 */

import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import type { UUID } from '@shared/CrossPlatformUUID';
import { type ProxyNavigateParams, type ProxyNavigateResult, createProxyNavigateParams } from '@commandsProxyNavigate/shared/ProxyNavigateTypes';

export abstract class ProxyNavigateCommand extends CommandBase<ProxyNavigateParams, ProxyNavigateResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('proxy-navigate', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ProxyNavigateParams {
    return createProxyNavigateParams(this.context, sessionId, {
      url: 'https://example.com'
    });
  }

  abstract execute(params: ProxyNavigateParams): Promise<ProxyNavigateResult>;
}