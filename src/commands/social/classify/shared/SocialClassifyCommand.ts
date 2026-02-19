import { CommandBase, type ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { SocialClassifyParams, SocialClassifyResult } from './SocialClassifyTypes';
import type { JTAGContext, JTAGPayload } from '@system/core/types/JTAGTypes';

export abstract class SocialClassifyBaseCommand extends CommandBase<SocialClassifyParams, SocialClassifyResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('social/classify', context, subpath, commander);
  }

  protected abstract executeSocialClassify(params: SocialClassifyParams): Promise<SocialClassifyResult>;

  async execute(params: JTAGPayload): Promise<SocialClassifyResult> {
    return this.executeSocialClassify(params as SocialClassifyParams);
  }
}
