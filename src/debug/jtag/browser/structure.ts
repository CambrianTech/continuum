/**
 * Browser Structure Registry - Static Imports
 * 
 * Contains only browser-side daemon and command imports.
 * No server imports to avoid Node.js dependencies in browser bundle.
 */

// Browser Daemon Imports  
import { CommandDaemonBrowser } from '../daemons/command-daemon/browser/CommandDaemonBrowser';
import { ConsoleDaemonBrowser } from '../daemons/console-daemon/browser/ConsoleDaemonBrowser';
import { HealthDaemonBrowser } from '../daemons/health-daemon/browser/HealthDaemonBrowser';

// Browser Command Imports
import { ScreenshotBrowserCommand } from '../daemons/command-daemon/commands/screenshot/browser/ScreenshotBrowserCommand';

// Chat Command Imports
// Chat commands temporarily disabled - god objects violating modular pattern
// import { SendMessageBrowserCommand } from '../daemons/command-daemon/commands/chat/send-message/browser/SendMessageBrowserCommand';
// import { GetChatHistoryBrowserCommand } from '../daemons/command-daemon/commands/chat/get-chat-history/browser/GetChatHistoryBrowserCommand';
// import { RoomEventBrowserCommand } from '../daemons/command-daemon/commands/chat/room-events/browser/RoomEventBrowserCommand';
// import { SendRoomEventBrowserCommand } from '../daemons/command-daemon/commands/chat/send-room-event/browser/SendRoomEventBrowserCommand';

// Types
import type { DaemonEntry } from '../shared/DaemonBase';
import type { CommandEntry } from '../daemons/command-daemon/shared/CommandBase';

/**
 * Browser Environment Registry
 */
export const BROWSER_DAEMONS: DaemonEntry[] = [
  {
    name: 'CommandDaemon',
    className: 'CommandDaemonBrowser', 
    daemonClass: CommandDaemonBrowser
  },
  {
    name: 'ConsoleDaemon',
    className: 'ConsoleDaemonBrowser',
    daemonClass: ConsoleDaemonBrowser
  },
  {
    name: 'HealthDaemon',
    className: 'HealthDaemonBrowser',
    daemonClass: HealthDaemonBrowser
  }
];

export const BROWSER_COMMANDS: CommandEntry[] = [
  {
    name: 'screenshot',
    className: 'ScreenshotBrowserCommand',
    commandClass: ScreenshotBrowserCommand
  },
  // Chat Commands - temporarily disabled (god objects violating modular pattern)
  /*
  {
    name: 'chat/send-message',
    className: 'SendMessageBrowserCommand',
    commandClass: SendMessageBrowserCommand
  },
  {
    name: 'chat/get-chat-history',
    className: 'GetChatHistoryBrowserCommand',
    commandClass: GetChatHistoryBrowserCommand
  },
  {
    name: 'chat/room-events',
    className: 'RoomEventBrowserCommand',
    commandClass: RoomEventBrowserCommand
  },
  {
    name: 'chat/send-room-event',
    className: 'SendRoomEventBrowserCommand',
    commandClass: SendRoomEventBrowserCommand
  }
  */
];


