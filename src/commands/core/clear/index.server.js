/**
 * Clear Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ClearCommand = require('./ClearCommand.cjs');

export default {
    command: ClearCommand,
    name: 'clear',
    version: '1.0.0',
    description: 'Clear command functionality',
    type: 'server',
    commands: ['clear'],
    initialize(environment = 'server') {
        console.log(`📦 Clear module (server) v1.0.0 initialized`);
        return true;
    }
};