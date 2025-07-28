import { CommandBase, type ICommandDaemon } from '@commandBase';
import type { JTAGContext } from '@shared/JTAGTypes';
import { UUID } from 'crypto';
import { type ScreenshotParams, type ScreenshotResult, createScreenshotParams } from '@commandsScreenshot/shared/ScreenshotTypes';

export abstract class ScreenshotCommand extends CommandBase<ScreenshotParams, ScreenshotResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('screenshot', context, subpath, commander);
  }

  public override getDefaultParams(sessionId: UUID): ScreenshotParams {
    return createScreenshotParams(this.context, sessionId, {
      filename: `screenshot-${new Date().toISOString().replace(/[:.]/g, '-')}.png`
    });
  }

  abstract execute(params: ScreenshotParams): Promise<ScreenshotResult>;
}