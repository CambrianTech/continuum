/**
 * Command Daemon - Browser Implementation
 * 
 * Browser-side command daemon that handles browser-specific commands.
 */

import type { JTAGContext } from '../../../shared/JTAGTypes';
import { CommandDaemon } from '../shared/CommandDaemon';
import type { CommandEntry } from '../shared/CommandBase';
import { BROWSER_COMMANDS } from './structure';
import type { CommandBase } from '../shared/CommandBase';

export class CommandDaemonBrowser extends CommandDaemon {

  protected override get commandEntries(): CommandEntry[] { return BROWSER_COMMANDS; }

  protected override createCommand(entry: CommandEntry, context: JTAGContext, subpath: string): CommandBase | null {
    return new entry.commandClass(context, subpath, this);
  }
}