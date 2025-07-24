/**
 * Command Daemon - Server Implementation
 * 
 * Server-side command daemon that handles server-specific commands.
 */


import type { JTAGContext } from '../../../shared/JTAGTypes';
import { CommandDaemon } from '../shared/CommandDaemon';
import type { CommandEntry } from '../shared/CommandBase';
import { SERVER_COMMANDS } from '../../../server/structure';
import type { CommandBase } from '../shared/CommandBase';

export class CommandDaemonServer extends CommandDaemon {
  
  protected override get commandEntries(): CommandEntry[] { return SERVER_COMMANDS; }
  
  protected override createCommand(entry: CommandEntry, context: JTAGContext, subpath: string): CommandBase | null {
      return new entry.commandClass(context, subpath, this);
  }
}