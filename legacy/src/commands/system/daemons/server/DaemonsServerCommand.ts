/**
 * Daemons Command - Server Implementation
 *
 * Lists all registered system daemons
 */

import { CommandBase, type ICommandDaemon } from '../../../../daemons/command-daemon/shared/CommandBase';
import type { JTAGContext } from '../../../../system/core/types/JTAGTypes';
import type { DaemonsParams, DaemonsResult, DaemonInfo } from '../shared/DaemonsTypes';
import { JTAGSystemServer } from '../../../../system/core/system/server/JTAGSystemServer';

export class DaemonsServerCommand extends CommandBase<DaemonsParams, DaemonsResult> {

  constructor(context: JTAGContext, subpath: string, commander: ICommandDaemon) {
    super('system/daemons', context, subpath, commander);
  }

  async execute(params: DaemonsParams): Promise<DaemonsResult> {
    console.log('📋 Listing system daemons...');

    // Access system daemons via JTAGSystemServer (same as ping command)
    const sys = JTAGSystemServer.instance;

    if (!sys) {
      return {
        context: params.context,
        sessionId: params.sessionId,
        success: false,
        error: 'JTAGSystemServer not available',
        daemons: [],
        total: 0,
        active: 0
      };
    }

    interface IDaemon {
      name: string;
    }

    const daemons = (sys.systemDaemons ?? []) as IDaemon[];
    const daemonsList: DaemonInfo[] = [];

    for (const daemon of daemons) {
      // Filter by name if specified
      if (params.nameFilter && !daemon.name.includes(params.nameFilter)) {
        continue;
      }

      daemonsList.push({
        name: daemon.name,
        status: 'active'  // All daemons in systemDaemons are active
      });
    }

    console.log(`✅ Found ${daemonsList.length} daemons (${daemonsList.length} active)`);

    return {
      context: params.context,
      sessionId: params.sessionId,
      success: true,
      daemons: daemonsList,
      total: daemonsList.length,
      active: daemonsList.length
    };
  }
}
