/**
 * ListAgents Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const ListAgentsCommand = require('./ListAgentsCommand.cjs');

export default {
    command: ListAgentsCommand,
    name: 'listAgents',
    version: '1.0.0',
    description: 'Agent discovery and status functionality',
    type: 'server',
    commands: ['listAgents'],
    initialize(environment = 'server') {
        console.log(`📦 ListAgents module (server) v1.0.0 initialized`);
        return true;
    }
};