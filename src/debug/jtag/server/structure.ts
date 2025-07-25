/**
 * Server Structure Registry - Static Imports
 * 
 * Contains only server-side daemon and command imports.
 * No browser imports to avoid bundling issues.
 */

// Server Daemon Imports
import { CommandDaemonServer } from '../daemons/command-daemon/server/CommandDaemonServer';
import { ConsoleDaemonServer } from '../daemons/console-daemon/server/ConsoleDaemonServer';
import { HealthDaemonServer } from '../daemons/health-daemon/server/HealthDaemonServer';

// Server Command Imports
import { ScreenshotServerCommand } from '../daemons/command-daemon/commands/screenshot/server/ScreenshotServerCommand';

// Chat Command Imports
// Chat commands temporarily disabled - god objects violating modular pattern
// import { SendMessageServerCommand } from '../daemons/command-daemon/commands/chat/send-message/server/SendMessageServerCommand';
// import { GetChatHistoryServerCommand } from '../daemons/command-daemon/commands/chat/get-chat-history/server/GetChatHistoryServerCommand';
// import { RoomEventServerCommand } from '../daemons/command-daemon/commands/chat/room-events/server/RoomEventServerCommand';
// import { SendRoomEventServerCommand } from '../daemons/command-daemon/commands/chat/send-room-event/server/SendRoomEventServerCommand';

// Types
import type { DaemonEntry } from '../shared/DaemonBase';
import type { CommandEntry } from '../daemons/command-daemon/shared/CommandBase';

/**
 * Server Environment Registry
 */
export const SERVER_DAEMONS: DaemonEntry[] = [
  {
    name: 'CommandDaemon',
    className: 'CommandDaemonServer',
    daemonClass: CommandDaemonServer
  },
  {
    name: 'ConsoleDaemon', 
    className: 'ConsoleDaemonServer',
    daemonClass: ConsoleDaemonServer
  },
  {
    name: 'HealthDaemon',
    className: 'HealthDaemonServer', 
    daemonClass: HealthDaemonServer
  }
];

export const SERVER_COMMANDS: CommandEntry[] = [
  {
    name: 'screenshot',
    className: 'ScreenshotServerCommand',
    commandClass: ScreenshotServerCommand
  },
  // Chat Commands - temporarily disabled (god objects violating modular pattern)
  /*
  {
    name: 'chat/send-message',
    className: 'SendMessageServerCommand',
    commandClass: SendMessageServerCommand
  },
  {
    name: 'chat/get-chat-history',
    className: 'GetChatHistoryServerCommand',
    commandClass: GetChatHistoryServerCommand
  },
  {
    name: 'chat/room-events',
    className: 'RoomEventServerCommand',
    commandClass: RoomEventServerCommand
  },
  {
    name: 'chat/send-room-event',
    className: 'SendRoomEventServerCommand',
    commandClass: SendRoomEventServerCommand
  }
  */
];