/**
 * Comms Test Command - Test daemon with echo and database modes
 */

import { CommandBase, type ICommandDaemon } from '../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext, JTAGPayload } from '../../../system/core/types/JTAGTypes';
import { transformPayload } from '../../../system/core/types/JTAGTypes';
import type { CommsTestParams, CommsTestResult } from '../shared/CommsTestTypes';
import { getCommsTestDaemon } from '../../../daemons/comms-test-daemon/server/CommsTestDaemonServer';

export class CommsTestServerCommand extends CommandBase<CommsTestParams, CommsTestResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('comms-test', context, subpath, commander);
  }

  async execute(params: JTAGPayload): Promise<CommsTestResult> {
    const testParams = params as CommsTestParams;
    const daemon = getCommsTestDaemon();

    // Echo mode
    if (testParams.mode === 'echo') {
      const result = await daemon.testEcho(testParams.message || 'hello');
      return transformPayload(testParams, {
        success: result.success,
        echo: result.echo
      });
    }

    // Database mode
    if (testParams.mode === 'database') {
      const dbParams = {
        dbCount: testParams.dbCount || 5,
        testDir: testParams.testDir || '.continuum/jtag/test-dbs',
        operations: testParams.operations || 10
      };

      const result = await daemon.testDatabase(dbParams);

      return transformPayload(testParams, {
        success: result.success,
        databases: result.databases,
        totalDuration: result.totalDuration,
        totalOperations: result.totalOperations
      });
    }

    // Invalid mode
    return transformPayload(testParams, {
      success: false,
      echo: `Invalid mode: ${testParams.mode}. Use 'echo' or 'database'.`
    });
  }
}
