/**
 * Scroll Test Server Command - Scroll Testing Orchestration
 *
 * Server-side orchestration for animated scroll testing with proper session handling.
 */

import { CommandBase } from '@daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '@system/core/types/JTAGTypes';
import type { ICommandDaemon } from '@daemons/command-daemon/shared/CommandBase';
import type { ScrollTestParams, ScrollTestResult } from '../shared/ScrollTestTypes';
import { createScrollTestResult } from '../shared/ScrollTestTypes';

export class ScrollTestServerCommand extends CommandBase<ScrollTestParams, ScrollTestResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('development/debug/scroll-test', context, subpath, commander);
  }

  async execute(params: ScrollTestParams): Promise<ScrollTestResult> {
    console.log('🔧 SCROLL-TEST-SERVER: Processing params:', params);

    // Handle preset shortcuts here as well (before validation)
    let effectiveParams = params;
    if (params.preset) {
      console.log('🔧 SCROLL-TEST-SERVER: Processing preset:', params.preset);
      switch (params.preset) {
        case 'chat-top':
          effectiveParams = { ...params, target: 'top', behavior: 'smooth', captureMetrics: true, waitTime: 1000 };
          break;
        case 'chat-bottom':
          effectiveParams = { ...params, target: 'bottom', behavior: 'smooth', captureMetrics: true, waitTime: 1000 };
          break;
        case 'instant-top':
          effectiveParams = { ...params, target: 'top', behavior: 'instant', captureMetrics: true };
          break;
      }
      console.log('🔧 SCROLL-TEST-SERVER: Effective params after preset:', effectiveParams);
    }

    // Validate parameters
    if (!effectiveParams.target || !['top', 'bottom', 'position'].includes(effectiveParams.target)) {
      console.log('🔧 SCROLL-TEST-SERVER: Invalid target:', effectiveParams.target);
      return createScrollTestResult(this.context, params.sessionId || 'unknown', {
        scrollPerformed: false,
        targetElement: 'unknown',
        initialPosition: 0,
        finalPosition: 0
      });
    }

    if (effectiveParams.target === 'position' && typeof effectiveParams.position !== 'number') {
      console.log('🔧 SCROLL-TEST-SERVER: Position target without valid position value');
      return createScrollTestResult(this.context, params.sessionId || 'unknown', {
        scrollPerformed: false,
        targetElement: 'unknown',
        initialPosition: 0,
        finalPosition: 0
      });
    }

    // For scroll testing, we need to execute on browser side
    console.log('🔧 SCROLL-TEST-SERVER: Calling remoteExecute with:', effectiveParams);
    return await this.remoteExecute(effectiveParams);
  }
}