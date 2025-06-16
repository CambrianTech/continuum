/**
 * Agents Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const AgentsCommand = require('./AgentsCommand.cjs');

export default {
    command: AgentsCommand,
    name: 'agents',
    version: '1.0.0',
    description: 'Agent management functionality',
    type: 'server',
    commands: ['agents'],
    initialize(environment = 'server') {
        console.log(`📦 Agents module (server) v1.0.0 initialized`);
        return true;
    }
};