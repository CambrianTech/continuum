/**
 * Help Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const HelpCommand = require('./HelpCommand.cjs');

export default {
    command: HelpCommand,
    name: 'help',
    version: '1.0.0',
    description: 'Help and documentation system',
    type: 'server',
    commands: ['help'],
    initialize(environment = 'server') {
        console.log(`📦 Help module (server) v1.0.0 initialized`);
        return true;
    }
};