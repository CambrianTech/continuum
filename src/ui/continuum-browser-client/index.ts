/**
 * Continuum Browser Client - Module Entry Point
 * Creates and exports the single global continuum instance
 */

import { ContinuumBrowserClient } from './ContinuumBrowserClient';

// Create single global instance
const continuum = new ContinuumBrowserClient();

// Expose as the ONLY global
(window as any).continuum = continuum;

console.log('🌐 Continuum Browser Client: Single global instance created');

export default continuum;
export { ContinuumBrowserClient };
export type { ContinuumAPI, ContinuumState, CommandResult } from './types/BrowserClientTypes';
export type { WebSocketMessage, ClientInitData, CommandExecuteData } from './types/WebSocketTypes';
export type { ConsoleCommand } from './types/ConsoleTypes';