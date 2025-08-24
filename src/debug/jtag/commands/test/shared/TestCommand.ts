// ISSUES: 0 open, last updated 2025-08-24 - See middle-out/development/code-quality-scouting.md#file-level-issue-tracking

/**
 * Test Command - Generic Base Class
 * 
 * Shared base class for test command implementations
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../system/core/types/JTAGTypes';
import type { UUID } from '../../../system/core/types/CrossPlatformUUID';
import { type TestParams, type TestResult, createTestParams } from './TestTypes';

export abstract class TestCommand extends CommandBase<TestParams, TestResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('test', context, subpath, commander);
  }

  public getDefaultParams(sessionId: UUID): TestParams {
    return createTestParams(this.context, sessionId, {
      timeout: 300000 // 5 minutes default
    });
  }

  abstract execute(params: TestParams): Promise<TestResult>;
}