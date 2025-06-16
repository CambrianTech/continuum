/**
 * Info Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const InfoCommand = require('./InfoCommand.cjs');

export default {
    command: InfoCommand,
    name: 'info',
    version: '1.0.0',
    description: 'System information and status',
    type: 'server',
    commands: ['info'],
    initialize(environment = 'server') {
        console.log(`📦 Info module (server) v1.0.0 initialized`);
        return true;
    }
};