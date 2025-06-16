/**
 * Diagnostics Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const DiagnosticsCommand = require('./DiagnosticsCommand.cjs');

export default {
    command: DiagnosticsCommand,
    name: 'diagnostics',
    version: '1.0.0',
    description: 'System diagnostics and health checks',
    type: 'server',
    commands: ['diagnostics'],
    initialize(environment = 'server') {
        console.log(`📦 Diagnostics module (server) v1.0.0 initialized`);
        return true;
    }
};