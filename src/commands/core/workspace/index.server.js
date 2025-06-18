/**
 * Workspace Command Module - Server Entry Point
 */
import { createRequire } from 'module';
const require = createRequire(import.meta.url);
const WorkspaceCommand = require('./WorkspaceCommand.cjs');

export default {
    command: WorkspaceCommand,
    name: 'workspace',
    version: '1.0.0',
    description: 'Workspace directory and path management',
    type: 'server',
    commands: ['workspace'],
    initialize(environment = 'server') {
        console.log(`📁 Workspace module (server) v1.0.0 initialized`);
        return true;
    }
};