/**
 * Reload Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ReloadCommand = require('./ReloadCommand.cjs');

export default {
    command: ReloadCommand,
    name: 'reload',
    version: '1.0.0',
    description: 'Reload system components',
    type: 'server',
    commands: ['reload'],
    initialize(environment = 'server') {
        console.log(`📦 Reload module (server) v1.0.0 initialized`);
        return true;
    }
};