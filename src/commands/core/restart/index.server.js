/**
 * Restart Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const RestartCommand = require('./RestartCommand.cjs');

export default {
    command: RestartCommand,
    name: 'restart',
    version: '1.0.0',
    description: 'System restart functionality',
    type: 'server',
    commands: ['restart'],
    initialize(environment = 'server') {
        console.log(`📦 Restart module (server) v1.0.0 initialized`);
        return true;
    }
};