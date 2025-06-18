/**
 * Sentinel Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const SentinelCommand = require('./SentinelCommand.cjs');

export default {
    command: SentinelCommand,
    name: 'sentinel',
    version: '1.0.0',
    description: 'AI Sentinel guardian agent for logging and task management',
    type: 'server',
    commands: ['sentinel'],
    initialize(environment = 'server') {
        console.log(`🛡️ Sentinel module (server) v1.0.0 initialized`);
        return true;
    }
};