import { CommandBase } from '../../../shared/CommandBase';
import type { ICommandDaemon } from '../../../shared/CommandBase';
import type { JTAGContext } from '../../../../../shared/JTAGTypes';
import type { ScreenshotParams } from './ScreenshotTypes';
import type { ScreenshotResult } from './ScreenshotTypes';

export abstract class ScreenshotCommand extends CommandBase {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot', context, subpath, commander);
  }

  abstract execute(params: ScreenshotParams): Promise<ScreenshotResult>;
}