import { CommandBase } from '../../../shared/CommandBase';
import type { ICommandDaemon } from '../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import { ScreenshotParams } from './ScreenshotTypes';
import type { ScreenshotResult } from './ScreenshotTypes';

export abstract class ScreenshotCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot', context, subpath, commander);
  }

  public override getDefaultParams(): ScreenshotParams {
    return new ScreenshotParams({
      filename: `screenshot-${new Date().toISOString().replace(/[:.]/g, '-')}.png`
    });
  }

  abstract execute(params: ScreenshotParams): Promise<ScreenshotResult>;
}