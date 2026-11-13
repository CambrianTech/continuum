/** Every <chat-widget> style sheet, in cascade order (later sheets win ties, exactly
 *  as the one block they were cut from did: the universes and the mobile rule stay last). */
import { shellStyles } from './shell';
import { consoleStyles } from './console';
import { personaTileStyles } from './personaTile';
import { transcriptStyles } from './transcript';
import { personaHomeStyles } from './personaHome';
import { settingsCanvasStyles } from './settingsCanvas';
import { liveCallStyles } from './liveCall';
import { universeStyles } from './universe';

export const chatWidgetStyles = [
  shellStyles,
  consoleStyles,
  personaTileStyles,
  transcriptStyles,
  personaHomeStyles,
  settingsCanvasStyles,
  liveCallStyles,
  universeStyles,
];
