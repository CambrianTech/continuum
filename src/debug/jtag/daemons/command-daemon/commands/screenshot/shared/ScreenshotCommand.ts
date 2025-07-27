import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { ScreenshotParams, type ScreenshotResult } from './ScreenshotTypes';

export abstract class ScreenshotCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ScreenshotParams {
    return new ScreenshotParams({
      filename: `screenshot-${new Date().toISOString().replace(/[:.]/g, '-')}.png`
    }, this.context, sessionId);
  }

  abstract execute(params: ScreenshotParams): Promise<ScreenshotResult>;
}