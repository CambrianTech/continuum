/**
 * Macro Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const MacroCommand = require('./MacroCommand.cjs');

export default {
    command: MacroCommand,
    name: 'macro',
    version: '1.0.0',
    description: 'Macro automation functionality',
    type: 'server',
    commands: ['macro'],
    initialize(environment = 'server') {
        console.log(`📦 Macro module (server) v1.0.0 initialized`);
        return true;
    }
};