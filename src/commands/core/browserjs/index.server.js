/**
 * BrowserJS Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const BrowserJSCommand = require('./BrowserJSCommand.cjs');

export default {
    command: BrowserJSCommand,
    name: 'browserJS',
    version: '1.0.0',
    description: 'Execute JavaScript in browser context',
    type: 'server',
    commands: ['browserJS'],
    initialize(environment = 'server') {
        console.log(`📦 BrowserJS module (server) v1.0.0 initialized`);
        return true;
    }
};