/**
 * Continuum Browser Client - Module Entry Point
 * Creates and exports the single global continuum instance
 */

import { ContinuumBrowserClient } from './ContinuumBrowserClient';
import { WidgetServerControls } from '../components/shared/WidgetServerControls';

// Trigger widget discovery plugin to automatically import all widgets
import 'widget-discovery';

// Create single global instance
const continuum = new ContinuumBrowserClient();

// Expose as the ONLY global
(window as any).continuum = continuum;

// Expose WidgetServerControls for dynamic widget-server integration
(window as any).WidgetServerControls = WidgetServerControls;

console.log('🌐 Continuum Browser Client: Single global instance created');
console.log('🎮 Widget Server Controls: Dynamic command discovery system initialized');

export default continuum;
export { ContinuumBrowserClient };
export type { ContinuumAPI, ContinuumState, CommandResult } from './types/BrowserClientTypes';
export type { WebSocketMessage, ClientInitData, CommandExecuteData } from './types/WebSocketTypes';
export type { ConsoleCommand } from './types/ConsoleTypes';