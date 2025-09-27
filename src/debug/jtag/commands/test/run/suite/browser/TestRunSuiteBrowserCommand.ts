/**
 * Test Run Suite Browser Command
 * Forwards test execution to server (tests run on Node.js)
 */

import { CommandBase } from '../../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../../system/core/types/JTAGTypes';
import type { ICommandDaemon } from '../../../../../daemons/command-daemon/shared/CommandBase';
import { TestRunSuiteParams, TestRunSuiteResult } from '../shared/TestRunSuiteTypes';

export class TestRunSuiteBrowserCommand extends CommandBase<TestRunSuiteParams, TestRunSuiteResult> {
  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('test-run-suite', context, subpath, commander);
  }

  async execute(params: TestRunSuiteParams): Promise<TestRunSuiteResult> {
    console.log('🌐 BROWSER: Forwarding test execution to server...');

    // Browser cannot execute Node.js tests directly
    // Forward to server implementation via command daemon
    // Browser forwards to server for test execution
    throw new Error('Browser test execution not implemented - use server environment');
  }

  getCommandName(): string {
    return 'test/run/suite';
  }

  getDescription(): string {
    return 'Run test suites by profile or custom configuration (forwarded to server)';
  }

  getUsageExamples(): string[] {
    return [
      'test/run/suite --profile="comprehensive"',
      'test/run/suite --profile="chat" --parallel --timeout=60000',
      'test/run/suite --tests="crud,chat,screenshots" --name="custom-precommit"',
      'test/run/suite --profile="integration" --format="json"'
    ];
  }
}