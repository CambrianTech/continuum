/**
 * Chat Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ChatCommand = require('./ChatCommand.cjs');

export default {
    command: ChatCommand,
    name: 'chat',
    version: '1.0.0',
    description: 'Multi-agent chat orchestration functionality',
    type: 'server',
    commands: ['chat'],
    initialize(environment = 'server') {
        console.log(`📦 Chat module (server) v1.0.0 initialized`);
        return true;
    }
};