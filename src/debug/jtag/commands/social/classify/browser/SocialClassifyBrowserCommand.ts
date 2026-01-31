import { SocialClassifyBaseCommand } from '../shared/SocialClassifyCommand';
import type { SocialClassifyParams, SocialClassifyResult } from '../shared/SocialClassifyTypes';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';

export class SocialClassifyBrowserCommand extends SocialClassifyBaseCommand {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super(context, subpath, commander);
  }

  protected async executeSocialClassify(params: SocialClassifyParams): Promise<SocialClassifyResult> {
    return await this.remoteExecute(params);
  }
}
