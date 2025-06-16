/**
 * Exec Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ExecCommand = require('./ExecCommand.cjs');

export default {
    command: ExecCommand,
    name: 'exec',
    version: '1.0.0',
    description: 'Execute system commands',
    type: 'server',
    commands: ['exec'],
    initialize(environment = 'server') {
        console.log(`📦 Exec module (server) v1.0.0 initialized`);
        return true;
    }
};