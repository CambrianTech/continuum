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

  /**
   * Override initialize to register to globalThis EARLY
   * This allows other daemons (like UserDaemon) to access CommandDaemon
   * during their initialization, before JTAGSystem.daemons is populated.
   */
  protected override async initialize(): Promise<void> {
    // First, do the normal command registration
    await super.initialize();

    // Register to globalThis IMMEDIATELY after commands are ready
    // This is critical for daemon initialization order - UserDaemon needs
    // CommandDaemon during its initializeDeferred(), which runs before
    // JTAGSystem.daemons array is populated by setupDaemons()
    (globalThis as any).__JTAG_COMMAND_DAEMON__ = this;
    this.log.info('✅ CommandDaemonServer: Registered to globalThis for early access');
  }

  protected override createCommand(entry: CommandEntry, context: JTAGContext, subpath: string): CommandBase<CommandParams, CommandResult> | null {
      return new entry.commandClass(context, subpath, this);
  }
}