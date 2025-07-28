/**
 * Command Daemon - Server Implementation
 * 
 * Server-side command daemon that handles server-specific commands.
 */


import type { JTAGContext, CommandParams, CommandResult } from '@shared/JTAGTypes';
import { CommandDaemon } from '@daemonsCommandDaemon/shared/CommandDaemon';
import type { CommandEntry } from '@commandBase';
import { SERVER_COMMANDS } from '@commands/server/generated';
import type { CommandBase } from '@commandBase';

export class CommandDaemonServer extends CommandDaemon {
  
  protected override get commandEntries(): CommandEntry[] { return SERVER_COMMANDS; }
  
  protected override createCommand(entry: CommandEntry, context: JTAGContext, subpath: string): CommandBase<CommandParams, CommandResult> | null {
      return new entry.commandClass(context, subpath, this);
  }
}