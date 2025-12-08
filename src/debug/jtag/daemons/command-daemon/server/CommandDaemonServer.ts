/**
 * Command Daemon - Server Implementation
 *
 * Server-side command daemon that handles server-specific commands.
 */


import type { JTAGContext, CommandParams, CommandResult } from '../../../system/core/types/JTAGTypes';
import type { JTAGRouter } from '../../../system/core/router/shared/JTAGRouter';
import { CommandDaemon } from '../shared/CommandDaemon';
import type { CommandEntry } from '../shared/CommandBase';
import { SERVER_COMMANDS } from '../../../server/generated';
import type { CommandBase } from '../shared/CommandBase';
import { Logger, type ComponentLogger } from '../../../system/core/logging/Logger';

export class CommandDaemonServer extends CommandDaemon {

  constructor(context: JTAGContext, router: JTAGRouter) {
    super(context, router);

    // Initialize standardized logging (daemons/ subdirectory)
    const className = this.constructor.name;
    this.log = Logger.create(className, `daemons/${className}`);
  }

  protected override get commandEntries(): CommandEntry[] { return SERVER_COMMANDS; }

  protected override createCommand(entry: CommandEntry, context: JTAGContext, subpath: string): CommandBase<CommandParams, CommandResult> | null {
      return new entry.commandClass(context, subpath, this);
  }
}