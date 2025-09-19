/**
 * Scroll Test Server Command - Scroll Testing Orchestration
 *
 * Server-side orchestration for animated scroll testing with proper session handling.
 */

import { CommandBase } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { ScrollTestParams, ScrollTestResult } from '../shared/ScrollTestTypes';
import { createScrollTestResult } from '../shared/ScrollTestTypes';

export class ScrollTestServerCommand extends CommandBase<ScrollTestParams, ScrollTestResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('scroll-test', context, subpath, commander);
  }

  async execute(params: ScrollTestParams): Promise<ScrollTestResult> {
    // Validate parameters
    if (!['top', 'bottom', 'position'].includes(params.target)) {
      return createScrollTestResult(this.context, params.sessionId || 'unknown', {
        scrollPerformed: false,
        targetElement: 'unknown',
        initialPosition: 0,
        finalPosition: 0
      });
    }

    if (params.target === 'position' && typeof params.position !== 'number') {
      return createScrollTestResult(this.context, params.sessionId || 'unknown', {
        scrollPerformed: false,
        targetElement: 'unknown',
        initialPosition: 0,
        finalPosition: 0
      });
    }

    // For scroll testing, we need to execute on browser side
    return await this.remoteExecute(params);
  }
}