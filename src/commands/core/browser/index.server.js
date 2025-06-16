/**
 * Browser Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const BrowserCommand = require('./BrowserCommand.cjs');

export default {
    command: BrowserCommand,
    name: 'browser',
    version: '1.0.0',
    description: 'Browser WebSocket coordination functionality',
    type: 'server',
    commands: ['browser'],
    initialize(environment = 'server') {
        console.log(`📦 Browser module (server) v1.0.0 initialized`);
        return true;
    }
};